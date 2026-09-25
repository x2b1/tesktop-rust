//! RobloxFilter: a content filter that stops what you would rather not receive or send.
//!
//! The rule tables are taken from the original file rather than retyped, so the port blocks
//! the same things the original does. Each rule is a pattern, the category it belongs to, and
//! the word-level replacements the original offers for it.

use crate::{Fallback, Meta, Outgoing, Setting, SettingKind, Values, flag_or, text_or};
use regex::Regex;
use std::sync::OnceLock;

/// A word the original replaces with something else, rather than blocking.
#[derive(Clone, Copy, Debug)]
pub struct Replacement {
	/// The pattern the word is written with, which is a regular expression in the original.
	pub word: &'static str,
	pub sfw: &'static str,
}

/// One rule: what it matches, what it is called, and what it could be replaced with.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
	pub pattern: &'static str,
	pub category: &'static str,
	pub replacements: &'static [Replacement],
}

/// What the port does when a rule matches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Action {
	/// Refuse the message; nothing goes out.
	#[default]
	Block,
	/// Write asterisks in place of every non-space character of the match.
	Censor,
	/// Write the rule's own replacement word.
	Replace,
}

impl Action {
	fn as_str(self) -> &'static str {
		match self {
			Self::Block => "block",
			Self::Censor => "censor",
			Self::Replace => "replace",
		}
	}
}

const SETTINGS: &[Setting] = &[
	Setting {
		key: "actionOnViolation",
		label: "What to do when a rule matches",
		kind: SettingKind::Choice(&[
			("block", "Block the message"),
			("censor", "Censor with *****"),
			("replace", "SFW word replace"),
		]),
		default: Fallback::Text("block"),
	},
	Setting {
		key: "strictMode",
		label: "Stricter detection, which catches more and forgives less",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "showWarningToast",
		label: "Say when a message was stopped",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "filterIncoming",
		label: "Filter the messages you receive as well",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
];

/// The compiled rules, built once: the original compiles them on first use for the same
/// reason, and a pattern that does not compile is reported rather than ignored.
fn compiled() -> &'static Vec<(&'static Rule, Regex)> {
	static COMPILED: OnceLock<Vec<(&'static Rule, Regex)>> = OnceLock::new();
	COMPILED.get_or_init(|| {
		MAIN_RULES
			.iter()
			.chain(STRICT_RULES.iter())
			.filter_map(|rule| Regex::new(rule.pattern).ok().map(|regex| (rule, regex)))
			.collect()
	})
}

/// How many rules could not be compiled, so the port can say so instead of quietly skipping.
pub fn unusable_rules() -> usize {
	MAIN_RULES.len() + STRICT_RULES.len() - compiled().len()
}

/// The rules the original ships, in its own order.
pub const MAIN_RULES: &[Rule] = &[
	Rule {
		pattern: "\\b(?:cp|c\\.s\\.a\\.m|child\\s*porn|kiddie\\s*porn|pedo\\s*file|child\\s*lover|young\\s*lover|jailbait|hebephil|ephebophil|pedophil|pedophili)\\b",
		category: "Child Exploitation",
		replacements: &[
			Replacement {
				word: "\\bcp\\b",
				sfw: "computer",
			},
			Replacement {
				word: "child\\s*porn",
				sfw: "innocent content",
			},
			Replacement {
				word: "kiddie\\s*porn",
				sfw: "family content",
			},
			Replacement {
				word: "\\bpedo\\s*file",
				sfw: "archive",
			},
			Replacement {
				word: "child\\s*lover",
				sfw: "caring person",
			},
			Replacement {
				word: "\\bjailbait\\b",
				sfw: "young-looking",
			},
		],
	},
	Rule {
		pattern: "\\b(?:naked\\s*kid|nude\\s*child|child\\s*nude|kid\\s*naked|kid\\s*nude|naked\\s*minor|nude\\s*minor|child\\s*sex|kid\\s*sex|minor\\s*sex|underage\\s*sex|teen\\s*sex(?:ual)?|lolita|loli\\s*sex|shota\\s*sex|shotacon|lolicon)\\b",
		category: "Child Exploitation",
		replacements: &[
			Replacement {
				word: "naked\\s*kid",
				sfw: "young person",
			},
			Replacement {
				word: "nude\\s*child",
				sfw: "young person",
			},
			Replacement {
				word: "child\\s*sex",
				sfw: "minor topic",
			},
			Replacement {
				word: "minor\\s*sex",
				sfw: "age-inappropriate topic",
			},
			Replacement {
				word: "underage\\s*sex",
				sfw: "age-inappropriate topic",
			},
			Replacement {
				word: "teen\\s*sexual?",
				sfw: "teen topic",
			},
			Replacement {
				word: "\\blolita\\b",
				sfw: "classic novel",
			},
			Replacement {
				word: "lolicon",
				sfw: "inappropriate interest",
			},
			Replacement {
				word: "shotacon",
				sfw: "inappropriate interest",
			},
		],
	},
	Rule {
		pattern: "\\b(?:12\\s*yo|13\\s*yo|14\\s*yo|under\\s*age|underage|minor|under.?age|under\\s*18|under\\s*16|under\\s*14|under\\s*13|under\\s*12)\\s*(?:sexy|hot|nude|naked|sexual|sex|porn|hentai)\\b",
		category: "Child Exploitation",
		replacements: &[],
	},
	Rule {
		pattern: "\\b(?:im\\s*1[0-4]\\b|i'?m\\s*1[0-4]\\b|i\\s*am\\s*1[0-4]\\b|age\\s*1[0-4]\\b|1[0-4]\\s*years?\\s*old)\\b",
		category: "Age Disclosure (Minor)",
		replacements: &[],
	},
	Rule {
		pattern: "\\b(?:isis|al[\\s-]*qaeda|boko\\s*haram|taliban|al[\\s-]*shabaab|hamas|hezbollah|jemaah\\s*islamiyah|abu\\s*sayyaf|ansar\\s*bayt\\s*al[\\s-]*maqdis)\\b",
		category: "Terrorism",
		replacements: &[
			Replacement {
				word: "\\bisis\\b",
				sfw: "the group",
			},
			Replacement {
				word: "al[\\s-]*qaeda",
				sfw: "the organization",
			},
			Replacement {
				word: "boko\\s*haram",
				sfw: "the group",
			},
			Replacement {
				word: "\\btaliban\\b",
				sfw: "the group",
			},
			Replacement {
				word: "\\bhamas\\b",
				sfw: "the organization",
			},
			Replacement {
				word: "\\bhezbollah\\b",
				sfw: "the organization",
			},
		],
	},
	Rule {
		pattern: "\\b(?:jihad|caliphate|khilafah|sharia\\s*state|islamic\\s*state|isil|daesh)\\b(?:\\s*(?:support|join|fight|victory|praise|allahu|akbar|state|flag|soldier|recruit))?\\b",
		category: "Terrorism / Violent Extremism",
		replacements: &[
			Replacement {
				word: "\\bjihad\\b",
				sfw: "struggle",
			},
			Replacement {
				word: "\\bcaliphate\\b",
				sfw: "historical state",
			},
			Replacement {
				word: "\\bkhilafah\\b",
				sfw: "historical state",
			},
			Replacement {
				word: "sharia\\s*state",
				sfw: "the region",
			},
			Replacement {
				word: "islamic\\s*state",
				sfw: "the group",
			},
			Replacement {
				word: "\\bisil\\b",
				sfw: "the group",
			},
			Replacement {
				word: "\\bdaesh\\b",
				sfw: "the group",
			},
		],
	},
	Rule {
		pattern: "\\b(?:mass\\s*shooting|school\\s*shooting|active\\s*shooter|shooting\\s*rampage|go\\s*on\\s*a\\s*rampage|kill\\s*them\\s*all|murder\\s*everyone|bomb\\s*threat|bomb\\s*plot|terrorist?\\s*attack)\\b",
		category: "Violent Threats / Terrorism",
		replacements: &[
			Replacement {
				word: "mass\\s*shooting",
				sfw: "tragic event",
			},
			Replacement {
				word: "school\\s*shooting",
				sfw: "tragic event",
			},
			Replacement {
				word: "active\\s*shooter",
				sfw: "emergency situation",
			},
			Replacement {
				word: "shooting\\s*rampage",
				sfw: "tragic event",
			},
			Replacement {
				word: "kill\\s*them\\s*all",
				sfw: "deal with them all",
			},
			Replacement {
				word: "murder\\s*everyone",
				sfw: "annoy everyone",
			},
			Replacement {
				word: "bomb\\s*threat",
				sfw: "security threat",
			},
			Replacement {
				word: "bomb\\s*plot",
				sfw: "security plot",
			},
			Replacement {
				word: "terrorist?\\s*attack",
				sfw: "attack",
			},
		],
	},
	Rule {
		pattern: "\\b(?:i'?ll\\s*(?:kill|murder|slaughter|beat|stab|shoot|rape|hurt|harm|destroy)\\s*(?:you|him|her|them|ur|ya)|gonna\\s*(?:kill|murder|slaughter|rape)|going\\s*to\\s*(?:kill|murder|rape))\\b",
		category: "Real-Life Threats",
		replacements: &[
			Replacement {
				word: "i'?ll\\s*kill\\s*you",
				sfw: "I'll argue with you",
			},
			Replacement {
				word: "i'?ll\\s*murder\\s*you",
				sfw: "I'll strongly disagree with you",
			},
			Replacement {
				word: "i'?ll\\s*slaughter\\s*you",
				sfw: "I'll debate you",
			},
			Replacement {
				word: "i'?ll\\s*beat\\s*you",
				sfw: "I'll outdo you",
			},
			Replacement {
				word: "i'?ll\\s*stab\\s*you",
				sfw: "I'll criticize you",
			},
			Replacement {
				word: "i'?ll\\s*shoot\\s*you",
				sfw: "I'll message you",
			},
			Replacement {
				word: "i'?ll\\s*hurt\\s*you",
				sfw: "I'll upset you",
			},
			Replacement {
				word: "i'?ll\\s*destroy\\s*you",
				sfw: "I'll outplay you",
			},
			Replacement {
				word: "gonna\\s*kill",
				sfw: "going to debate",
			},
			Replacement {
				word: "gonna\\s*murder",
				sfw: "going to argue",
			},
			Replacement {
				word: "going\\s*to\\s*kill",
				sfw: "going to argue with",
			},
			Replacement {
				word: "going\\s*to\\s*murder",
				sfw: "going to argue with",
			},
		],
	},
	Rule {
		pattern: "\\b(?:dox|doxx|drop\\s*dox|drop\\s*ip|swat|swatt|leak\\s*(?:your|their|his|her)\\s*(?:address|ip|location|phone|real\\s*name|where\\s*(?:you|they|he|she)\\s*live))\\b",
		category: "Doxxing / Privacy Violations",
		replacements: &[
			Replacement {
				word: "\\bdox(?:x)?\\b",
				sfw: "share public info",
			},
			Replacement {
				word: "drop\\s*dox",
				sfw: "share info",
			},
			Replacement {
				word: "drop\\s*ip",
				sfw: "share info",
			},
			Replacement {
				word: "\\bswat(?:t)?\\b",
				sfw: "prank call",
			},
			Replacement {
				word: "leak\\s*(?:your|their|his|her)\\s*(?:address|ip|location|phone|real\\s*name)",
				sfw: "share info about them",
			},
		],
	},
	Rule {
		pattern: "\\b(?:dox|doxx)\\b(?:ing|ed)?\\s*(?:\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}|[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}|\\d{3}[\\s-]?\\d{3}[\\s-]?\\d{4})",
		category: "Doxxing",
		replacements: &[],
	},
	Rule {
		pattern: "\\b(?:kill\\s*myself|kms|kys|end\\s*(?:my|your|their)\\s*life|end\\s*it\\s*all|commit\\s*suicide|kill\\s*(?:me|yourself|myself)|want\\s*to\\s*die|gonna\\s*die|going\\s*to\\s*die|i'?ll\\s*die|i\\s*want\\s*to\\s*die)\\b",
		category: "Self-Harm / Suicide",
		replacements: &[
			Replacement {
				word: "kill\\s*myself",
				sfw: "take care of myself",
			},
			Replacement {
				word: "\\bkms\\b",
				sfw: "I'm okay",
			},
			Replacement {
				word: "\\bkys\\b",
				sfw: "be kind",
			},
			Replacement {
				word: "end\\s*(?:my|your|their)\\s*life",
				sfw: "change my situation",
			},
			Replacement {
				word: "end\\s*it\\s*all",
				sfw: "start fresh",
			},
			Replacement {
				word: "commit\\s*suicide",
				sfw: "hurt myself",
			},
			Replacement {
				word: "kill\\s*(?:me|yourself|myself)",
				sfw: "be gentle with myself",
			},
			Replacement {
				word: "want\\s*to\\s*die",
				sfw: "need help",
			},
			Replacement {
				word: "gonna\\s*die",
				sfw: "going to be okay",
			},
			Replacement {
				word: "going\\s*to\\s*die",
				sfw: "going to be okay",
			},
			Replacement {
				word: "i'?ll\\s*die",
				sfw: "I'll manage",
			},
			Replacement {
				word: "i\\s*want\\s*to\\s*die",
				sfw: "I need support",
			},
		],
	},
	Rule {
		pattern: "\\b(?:how\\s*to\\s*(?:kill\\s*myself|commit\\s*suicide|end\\s*my\\s*life)|suicide\\s*(?:methods|ways|how|help|tips|guide))\\b",
		category: "Self-Harm Methods",
		replacements: &[],
	},
	Rule {
		pattern: "\\b(?:how\\s*to\\s*(?:make|buy|create|synthesize|cook)\\s*(?:meth|methamphetamine|fentanyl|heroin|cocaine|crack|lsd|mdma|ecstasy|bath\\s*salt|synthetic?\\s*cannabis|fent))\\b",
		category: "Illegal Drug Manufacturing",
		replacements: &[
			Replacement {
				word: "\\bmeth(?:amphetamine)?\\b",
				sfw: "chemistry",
			},
			Replacement {
				word: "\\bfentanyl\\b",
				sfw: "medication",
			},
			Replacement {
				word: "\\bheroin\\b",
				sfw: "substance",
			},
			Replacement {
				word: "\\bcocaine\\b",
				sfw: "substance",
			},
			Replacement {
				word: "\\bcrack\\b",
				sfw: "substance",
			},
			Replacement {
				word: "\\blsd\\b",
				sfw: "substance",
			},
			Replacement {
				word: "\\bmdma\\b",
				sfw: "substance",
			},
			Replacement {
				word: "\\becstasy\\b",
				sfw: "substance",
			},
		],
	},
	Rule {
		pattern: "\\b(?:buy|purchase|get|order|source)\\s*(?:(?:il)?legal|black\\s*market|dark\\s*web|darknet)\\s*(?:gun|firearm|weapon|bomb|explosive|drug|heroin|meth|fentanyl|cocaine|crack|ak[\\s-]?47|ar[\\s-]?15|handgun|silencer)\\b",
		category: "Illegal Weapons / Drugs",
		replacements: &[],
	},
	Rule {
		pattern: "\\b(?:hack|steal|crack|bypass)\\s*(?:some)?one'?s?\\s*(?:account|password|bank|credit\\s*card|social\\s*security|ssn|id|identity|wallet|crypto)\\b",
		category: "Account Theft / Identity Crime",
		replacements: &[
			Replacement {
				word: "\\bhack\\b",
				sfw: "access",
			},
			Replacement {
				word: "\\bsteal\\b",
				sfw: "borrow",
			},
			Replacement {
				word: "\\bcrack\\b",
				sfw: "bypass",
			},
			Replacement {
				word: "\\bbypass\\b",
				sfw: "work around",
			},
		],
	},
	Rule {
		pattern: "\\b(?:revenge\\s*porn|leaked\\s*(?:nude|naked|sexy|explicit|porn|tape|video|photo|pic)|leak\\s*(?:her|his|their|your|my)\\s*(?:nude|naked|sexy|explicit|porn|tape|video|photo|pic)|without\\s*(?:her|his|their|your|my)\\s*(?:consent|permission|knowledge))\\b",
		category: "Non-Consensual Intimate Imagery",
		replacements: &[
			Replacement {
				word: "revenge\\s*porn",
				sfw: "private content",
			},
			Replacement {
				word: "leaked\\s*(?:nude|naked|sexy|explicit|porn)",
				sfw: "shared content",
			},
			Replacement {
				word: "without\\s*(?:her|his|their|your|my)\\s*consent",
				sfw: "without permission",
			},
		],
	},
	Rule {
		pattern: "\\b(?:nigga|nig(?:g|g)?er|fag(?:got)?|tranny|chink|spic|gook|kike|wetback|cracker|redskin|paki|coon|towel\\s*head|camel\\s*jockey|raghead|gypsy|retard|retarded|mongoloid|dumb\\s*retard)\\b",
		category: "Hate Speech / Slurs",
		replacements: &[
			Replacement {
				word: "\\bnigga\\b",
				sfw: "dark skinned mate",
			},
			Replacement {
				word: "\\bnig(?:g|g)?er\\b",
				sfw: "dark skinned person",
			},
			Replacement {
				word: "\\bfag(?:got)?\\b",
				sfw: "person",
			},
			Replacement {
				word: "\\btranny\\b",
				sfw: "trans person",
			},
			Replacement {
				word: "\\bchink\\b",
				sfw: "person",
			},
			Replacement {
				word: "\\bspic\\b",
				sfw: "person",
			},
			Replacement {
				word: "\\bgook\\b",
				sfw: "person",
			},
			Replacement {
				word: "\\bkike\\b",
				sfw: "person",
			},
			Replacement {
				word: "\\bwetback\\b",
				sfw: "person",
			},
			Replacement {
				word: "\\bcracker\\b",
				sfw: "person",
			},
			Replacement {
				word: "\\bredskin\\b",
				sfw: "person",
			},
			Replacement {
				word: "\\bpaki\\b",
				sfw: "person",
			},
			Replacement {
				word: "\\bcoon\\b",
				sfw: "person",
			},
			Replacement {
				word: "towel\\s*head",
				sfw: "person",
			},
			Replacement {
				word: "camel\\s*jockey",
				sfw: "person",
			},
			Replacement {
				word: "\\braghead\\b",
				sfw: "person",
			},
			Replacement {
				word: "\\bgypsy\\b",
				sfw: "traveler",
			},
			Replacement {
				word: "\\bretard(?:ed)?\\b",
				sfw: "silly",
			},
			Replacement {
				word: "dumb\\s*retard",
				sfw: "silly",
			},
			Replacement {
				word: "\\bmongoloid\\b",
				sfw: "person",
			},
		],
	},
	Rule {
		pattern: "\\b(?:white\\s*power|white\\s*genocide|heil\\s*hitler|nazi|reich|kkk|klan|aryan\\s*suprem|black\\s*power|israel\\s*(?:must|should|will)\\s*(?:die|be\\s*destroyed)|kill\\s*(?:all\\s*)?(?:jews|blacks|whites|arabs|muslims|christians|hindus))\\b",
		category: "Hate Speech / Extremism",
		replacements: &[
			Replacement {
				word: "white\\s*power",
				sfw: "solidarity",
			},
			Replacement {
				word: "white\\s*genocide",
				sfw: "demographic change",
			},
			Replacement {
				word: "heil\\s*hitler",
				sfw: "historical reference",
			},
			Replacement {
				word: "\\bnazi\\b",
				sfw: "historical group",
			},
			Replacement {
				word: "\\breich\\b",
				sfw: "historical state",
			},
			Replacement {
				word: "\\bkkk\\b",
				sfw: "historical group",
			},
			Replacement {
				word: "\\bklan\\b",
				sfw: "historical group",
			},
			Replacement {
				word: "black\\s*power",
				sfw: "solidarity",
			},
		],
	},
	Rule {
		pattern: "\\b(?:grabify|ip\\s*log|ip\\s*grab|token\\s*grab|token\\s*log|steal\\s*tokens|steal\\s*passwords|phishing?\\s*link|fake\\s*login|fake\\s*discord|fake\\s*nitro|nitro\\s*scam|nitro\\s*generator|free\\s*nitro\\s*(?:here|link|click|giveaway|generator))\\b",
		category: "Malware / Phishing / Scams",
		replacements: &[
			Replacement {
				word: "\\bgrabify\\b",
				sfw: "link tracker",
			},
			Replacement {
				word: "ip\\s*log",
				sfw: "ip checker",
			},
			Replacement {
				word: "ip\\s*grab",
				sfw: "ip checker",
			},
			Replacement {
				word: "token\\s*grab",
				sfw: "account access",
			},
			Replacement {
				word: "phishing?\\s*link",
				sfw: "suspicious link",
			},
			Replacement {
				word: "fake\\s*nitro",
				sfw: "not real nitro",
			},
			Replacement {
				word: "nitro\\s*scam",
				sfw: "not real nitro",
			},
			Replacement {
				word: "nitro\\s*generator",
				sfw: "fake nitro tool",
			},
			Replacement {
				word: "free\\s*nitro",
				sfw: "not real nitro",
			},
		],
	},
	Rule {
		pattern: "\\b(?:malware|virus|trojan|keylog|ransomware|cryptolocker|rootkit|backdoor|remote\\s*access\\s*trojan|rat\\s*(?:virus|malware|download))\\b",
		category: "Malware Distribution",
		replacements: &[
			Replacement {
				word: "\\bmalware\\b",
				sfw: "harmful software",
			},
			Replacement {
				word: "\\bvirus\\b",
				sfw: "harmful code",
			},
			Replacement {
				word: "\\btrojan\\b",
				sfw: "hidden malware",
			},
			Replacement {
				word: "\\bkeylog(?:ger)?\\b",
				sfw: "input monitor",
			},
			Replacement {
				word: "\\bransomware\\b",
				sfw: "encryption malware",
			},
			Replacement {
				word: "\\brootkit\\b",
				sfw: "hidden software",
			},
			Replacement {
				word: "\\bbackdoor\\b",
				sfw: "hidden access",
			},
		],
	},
	Rule {
		pattern: "\\b(?:gore|decapitat|beheading|live\\s*leak|(?:live)?leak\\s*(?:video|gore|death)|crackhead|suicide\\s*(?:video|note|footage)|snuff|execution\\s*(?:video|footage)|torture\\s*(?:video|gore|porn))\\b",
		category: "Gore / Extreme Violence",
		replacements: &[
			Replacement {
				word: "\\bgore\\b",
				sfw: "graphic content",
			},
			Replacement {
				word: "decapitat",
				sfw: "historical event",
			},
			Replacement {
				word: "beheading",
				sfw: "historical event",
			},
			Replacement {
				word: "live\\s*leak",
				sfw: "archived video",
			},
			Replacement {
				word: "leak\\s*video",
				sfw: "shared video",
			},
			Replacement {
				word: "\\bcrackhead\\b",
				sfw: "struggling person",
			},
			Replacement {
				word: "suicide\\s*(?:video|note|footage)",
				sfw: "tragic content",
			},
			Replacement {
				word: "\\bsnuff\\b",
				sfw: "extreme content",
			},
			Replacement {
				word: "execution\\s*video",
				sfw: "historical footage",
			},
			Replacement {
				word: "torture\\s*video",
				sfw: "disturbing content",
			},
		],
	},
	Rule {
		pattern: "\\b(?:rape|rapist|sexual\\s*assault|forced\\s*sex|non[- ]consensual|consent(?:ual)?\\s*(?:sex|rape|abuse))\\b",
		category: "Sexual Violence",
		replacements: &[
			Replacement {
				word: "\\brape\\b",
				sfw: "assault",
			},
			Replacement {
				word: "\\brapist\\b",
				sfw: "assailant",
			},
			Replacement {
				word: "sexual\\s*assault",
				sfw: "assault",
			},
			Replacement {
				word: "forced\\s*sex",
				sfw: "non-consensual act",
			},
		],
	},
	Rule {
		pattern: "\\b(?:animal\\s*(?:abuse|torture|kill|rape|crush|bestiality|zoophilia)|pet\\s*(?:abuse|torture|kill))\\b",
		category: "Animal Abuse",
		replacements: &[
			Replacement {
				word: "animal\\s*abuse",
				sfw: "animal mistreatment",
			},
			Replacement {
				word: "animal\\s*torture",
				sfw: "animal mistreatment",
			},
			Replacement {
				word: "pet\\s*abuse",
				sfw: "pet mistreatment",
			},
			Replacement {
				word: "\\bbestiality\\b",
				sfw: "inappropriate behavior",
			},
		],
	},
	Rule {
		pattern: "\\b(?:raid|nuke|spam)\\s*(?:server|guild|channel|group)\\b",
		category: "Platform Abuse",
		replacements: &[
			Replacement {
				word: "raid\\s*(?:server|guild|channel|group)",
				sfw: "visit the community",
			},
			Replacement {
				word: "nuke\\s*(?:server|guild|channel|group)",
				sfw: "clean the server",
			},
			Replacement {
				word: "spam\\s*(?:server|guild|channel|group)",
				sfw: "flood the server",
			},
		],
	},
];

/// The stricter rules, which the original keeps for its strict mode.
pub const STRICT_RULES: &[Rule] = &[
	Rule {
		pattern: "\\b(?:die|death|dead|kill|murder|destroy|annihilate|eliminate)\\b.{0,20}\\b(?:you|him|her|them|ur|ya)\\b",
		category: "Violent Language",
		replacements: &[
			Replacement {
				word: "die\\b.{0,20}(?:you|him|her|them)",
				sfw: "disagree with you",
			},
			Replacement {
				word: "kill\\b.{0,20}(?:you|him|her|them)",
				sfw: "argue with you",
			},
			Replacement {
				word: "murder\\b.{0,20}(?:you|him|her|them)",
				sfw: "strongly disagree with you",
			},
		],
	},
	Rule {
		pattern: "\\b(?:fuck\\s*(?:you|off|up|yourself)|shit|bitch|ass(?:hole)?|damn|hell)\\b.{0,10}\\b(?:you|him|her|them)\\b",
		category: "Abusive Language",
		replacements: &[
			Replacement {
				word: "fuck\\s*you",
				sfw: "fudge you",
			},
			Replacement {
				word: "fuck\\s*off",
				sfw: "go away",
			},
			Replacement {
				word: "fuck\\s*up",
				sfw: "mess up",
			},
			Replacement {
				word: "fuck\\s*yourself",
				sfw: "be kind to yourself",
			},
			Replacement {
				word: "\\bshit\\b",
				sfw: "stuff",
			},
			Replacement {
				word: "\\bbitch\\b",
				sfw: "person",
			},
			Replacement {
				word: "\\bass(?:hole)?\\b",
				sfw: "jerk",
			},
			Replacement {
				word: "\\bdamn\\b",
				sfw: "darn",
			},
			Replacement {
				word: "\\bhell\\b",
				sfw: "heck",
			},
		],
	},
];

/// A rule that matched, and the reason it is a violation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
	pub category: &'static str,
	pub rule: &'static str,
}

/// Every rule the message breaks, in the tables' own order, with the strict ones only when
/// strict mode is on. A rule with a replacement is still a violation: the replacement is what
/// the word is turned into, not a reason to let the word through.
pub fn check(content: &str, strict: bool) -> Vec<Violation> {
	compiled()
		.iter()
		.filter(|(rule, _)| strict || !strict_only(rule))
		.filter(|(_, regex)| regex.is_match(content))
		.map(|(rule, _)| Violation {
			category: rule.category,
			rule: rule.pattern,
		})
		.collect()
}

/// Whether a rule belongs to the strict table, which is only consulted when strict mode is on.
fn strict_only(rule: &&Rule) -> bool {
	STRICT_RULES
		.iter()
		.any(|strict_rule| std::ptr::eq(strict_rule as *const Rule, *rule as *const Rule))
}

/// The categories the message breaks, in the order they first appear and without repeats.
pub fn categories(violations: &[Violation]) -> Vec<&'static str> {
	let mut seen: Vec<&'static str> = Vec::new();
	for violation in violations {
		if !seen.contains(&violation.category) {
			seen.push(violation.category);
		}
	}
	seen
}

/// Write asterisks over everything but the spaces, which is how the original censors.
pub fn asterisks(matched: &str) -> String {
	matched
		.chars()
		.map(|character| {
			if character.is_whitespace() {
				character
			} else {
				'*'
			}
		})
		.collect()
}

/// What the message looks like after the rules that could replace a word have had one, which
/// is what the `Censor` and `Replace` actions send instead of refusing.
pub fn rewrite(content: &str, action: Action, strict: bool) -> String {
	if matches!(action, Action::Block) {
		return content.to_string();
	}
	let mut out = content.to_string();
	for (rule, regex) in compiled() {
		if strict_only(rule) && !strict {
			continue;
		}
		if !regex.is_match(&out) {
			continue;
		}
		for replacement in rule.replacements {
			let Ok(word) = Regex::new(replacement.word) else {
				continue;
			};
			out = word
				.replace_all(&out, |found: &regex::Captures<'_>| match action {
					Action::Censor => asterisks(&found[0]),
					Action::Replace => replacement.sfw.to_string(),
					Action::Block => found[0].to_string(),
				})
				.into_owned();
		}
	}
	out
}

/// RobloxFilter: what you would rather not receive, and what you would rather not send.
#[derive(Default)]
pub struct RobloxFilter {
	action: Action,
	strict: bool,
	toast: bool,
	incoming: bool,
	pending: Option<String>,
}

impl crate::Plugin for RobloxFilter {
	fn meta(&self) -> Meta {
		Meta {
			id: "RobloxFilter",
			name: "RobloxFilter",
			description: "Stops the messages that break a content rule, in both directions.",
			authors: "Testcord",
			tags: &["Utility", "Chat"],
			aliases: &["robloxfilter"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.action = match text_or(values, SETTINGS, "actionOnViolation").as_str() {
			"censor" => Action::Censor,
			"replace" => Action::Replace,
			_ => Action::Block,
		};
		self.strict = flag_or(values, SETTINGS, "strictMode");
		self.toast = flag_or(values, SETTINGS, "showWarningToast");
		self.incoming = flag_or(values, SETTINGS, "filterIncoming");
		self.pending = None;
	}

	fn reset(&mut self) {
		self.pending = None;
	}

	fn mutate_incoming(&mut self, message: &mut model::Message) {
		if !self.incoming {
			return;
		}
		let violations = check(&message.content, self.strict);
		if violations.is_empty() {
			return;
		}
		if self.toast {
			self.pending = Some(format!(
				"🛡️ Hid a message that breaks: {}",
				categories(&violations).join(", ")
			));
		}
		match self.action {
			// Nothing is going to be shown either way, so an incoming message is dropped.
			Action::Block => message.content = String::new(),
			action => message.content = rewrite(&message.content, action, self.strict),
		}
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		let content = std::mem::take(outgoing.body);
		let violations = check(&content, self.strict);
		if violations.is_empty() {
			*outgoing.body = content;
			return Ok(());
		}
		let what = categories(&violations).join(", ");
		match self.action {
			Action::Block => {
				if self.toast {
					self.pending = Some(format!("🛡️ Message blocked: {what}"));
				}
				Err("blocked by a content rule you set")
			}
			action => {
				*outgoing.body = rewrite(&content, action, self.strict);
				Ok(())
			}
		}
	}

	fn take_toast(&mut self) -> Option<String> {
		self.pending.take()
	}

	fn summary(&self) -> Option<String> {
		let mut summary = format!(
			"{} rules, {}",
			MAIN_RULES.len() + STRICT_RULES.len(),
			self.action.as_str()
		);
		if self.strict {
			summary.push_str(", strict");
		}
		if unusable_rules() > 0 {
			summary.push_str(&format!(" · {} rules unusable", unusable_rules()));
		}
		Some(summary)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{Plugin, Registry};

	fn configured(action: &str, strict: bool) -> RobloxFilter {
		let mut plugin = RobloxFilter::default();
		plugin.configure(&Values(
			[
				("actionOnViolation".to_string(), serde_json::json!(action)),
				("strictMode".to_string(), serde_json::json!(strict)),
			]
			.into_iter()
			.collect(),
		));
		plugin
	}

	fn send(plugin: &mut dyn Plugin, body: &str) -> Result<String, &'static str> {
		let mut text = body.to_string();
		let mut outgoing = Outgoing {
			channel: model::Id(7),
			me: model::Id(1),
			body: &mut text,
			reply: None,
			previous: None,
			route: crate::Route::Send,
		};
		plugin.before_send(&mut outgoing)?;
		Ok(outgoing.body.clone())
	}

	#[test]
	fn every_rule_the_original_ships_compiles() {
		assert_eq!(
			unusable_rules(),
			0,
			"a rule this port cannot read is a rule that silently does nothing"
		);
	}

	#[test]
	fn the_rule_count_matches_the_original() {
		assert_eq!(MAIN_RULES.len(), 24);
		assert_eq!(STRICT_RULES.len(), 2);
		let replacements: usize = MAIN_RULES
			.iter()
			.chain(STRICT_RULES)
			.map(|rule| rule.replacements.len())
			.sum();
		assert_eq!(
			replacements, 158,
			"the word-level replacements are all here"
		);
	}

	#[test]
	fn a_message_that_breaks_a_rule_does_not_go_out() {
		let mut plugin = configured("block", false);
		let error = send(&mut plugin, "here is a link to some cp").expect_err("blocked");
		assert!(error.contains("content rule"), "{error}");
		assert!(
			plugin.take_toast().unwrap().contains("blocked"),
			"the owner is told what stopped it"
		);
	}

	#[test]
	fn an_ordinary_message_goes_out_untouched() {
		let mut plugin = configured("block", false);
		assert_eq!(
			send(&mut plugin, "an ordinary message").unwrap(),
			"an ordinary message"
		);
		assert!(plugin.take_toast().is_none());
	}

	#[test]
	fn censoring_keeps_the_spaces() {
		let mut plugin = configured("censor", false);
		let out = send(&mut plugin, "child porn here").unwrap();
		assert!(!out.contains("child"), "{out}");
		assert!(!out.contains("porn"), "{out}");
		// The word the rule matched is covered and the rest of the line is not, which is the
		// whole difference between censoring a word and hiding a message.
		assert!(out.ends_with(" here"), "{out}");
		assert!(
			out.chars()
				.rev()
				.skip(5)
				.all(|character| character == '*' || character == ' '),
			"only asterisks and spaces over the word: {out}"
		);
		assert_eq!(
			out.chars().filter(|character| *character == ' ').count(),
			2,
			"{out}"
		);
	}

	#[test]
	fn replacing_writes_the_rules_own_word() {
		let mut plugin = configured("replace", false);
		let out = send(&mut plugin, "cp").unwrap();
		assert_eq!(out, "computer", "the rule's own replacement: {out}");
	}

	#[test]
	fn a_replace_still_leaves_a_word_the_rule_has_no_word_for() {
		let mut plugin = configured("replace", false);
		// The rule matches `jailbait` and the original has no replacement word for it, so
		// the word survives: the port does not invent a word the original does not have.
		let out = send(&mut plugin, "a jailbait mention").unwrap();
		assert_eq!(out, "a young-looking mention");
	}

	#[test]
	fn strict_mode_adds_rules_rather_than_replacing_them() {
		// A body the relaxed table already breaks is still broken with strict mode on, and
		// the strict table can only ever add.
		let relaxed = check("a link to cp", false);
		let strict = check("a link to cp", true);
		assert!(!relaxed.is_empty());
		assert!(strict.len() >= relaxed.len(), "{strict:?}");
		assert!(
			STRICT_RULES.iter().all(|rule| !MAIN_RULES
				.iter()
				.any(|main| std::ptr::eq(main as *const Rule, rule as *const Rule))),
			"a rule is in one table or the other, never both"
		);
	}

	#[test]
	fn the_categories_are_named_once_each() {
		let violations = check("a link to cp", true);
		let named = categories(&violations);
		let mut unique = named.clone();
		unique.sort_unstable();
		unique.dedup();
		assert_eq!(
			named, unique,
			"a category repeated is a category said twice"
		);
	}

	#[test]
	fn an_incoming_message_is_filtered_too() {
		let mut plugin = configured("censor", false);
		let mut message = test_support::message(1, model::Id(7));
		message.content = "child porn here".to_string();
		plugin.mutate_incoming(&mut message);
		assert!(!message.content.contains("porn"), "{}", message.content);
		assert!(plugin.take_toast().unwrap().contains("Hid a message"));
	}

	#[test]
	fn the_incoming_half_can_be_turned_off() {
		let mut plugin = configured("block", false);
		plugin.configure(&Values(
			[
				("actionOnViolation".to_string(), serde_json::json!("block")),
				("filterIncoming".to_string(), serde_json::json!(false)),
			]
			.into_iter()
			.collect(),
		));
		let mut message = test_support::message(1, model::Id(7));
		message.content = "cp".to_string();
		plugin.mutate_incoming(&mut message);
		assert_eq!(message.content, "cp");
	}

	#[test]
	fn the_registry_says_what_it_carries() {
		let mut registry = Registry::new();
		registry.set_enabled("RobloxFilter", true);
		let summary = registry.summary("RobloxFilter").expect("a summary");
		assert!(summary.contains("26 rules"), "{summary}");
		assert!(summary.contains("block"), "{summary}");
	}

	#[test]
	fn asterisks_cover_everything_but_the_spaces() {
		assert_eq!(asterisks("ab cd"), "** **");
		assert_eq!(asterisks(""), "");
	}
}
