# Bundled TestCord plugins

tesktop2 ships native Rust ports of TestCord plugins. This page records what the runtime
guarantees, what is ported today, and what a port may and may not do.

## Why a separate runtime

The sandboxed community extension runtime in `crates/extensions` observes messages and asks the
app to act, but it has no hook that can rewrite or veto the owner's own outgoing body and no hook
that can keep a message out of the timeline. Those two hooks are what TestCord's most used
plugins are built on, so `crates/tesktop-plugins` provides them natively. Read-only and
UI-shaped community add-ons still belong on the Extensions page, sandboxed.

Everything here runs on the UI thread next to the state owner. There is no plugin process, no
script engine and no network call on the message path.

## The contract

A bundled plugin declares a `Meta` (id, name, description, authors, tags, aliases, default
state) and may declare `Setting`s, each with one declared default that both the plugin and the
settings page read. The registry owns the enable flag, the stored values and the plugin's
remembered state.

| Hook | Fires on | Plugin may |
|---|---|---|
| `mutate_incoming` | An accepted inbound message, before it is stored | Take pings or other content out of it |
| `ignore` | The same message, after `mutate_incoming` | Hide it, so it never enters the timeline |
| `on_created` / `on_edited` / `on_deleted` | The same events, after `ignore` | Record them, queue a reply |
| `before_send` / `before_edit` | Every outgoing body and its reply mention | Rewrite either, or refuse it with a reason |
| `command` / `command_names` | A typed line that starts with `/` | Expand a slash command into the body to send, and name the commands for the composer's list |
| `route` | The same body, before it is split | Claim the send so the host edits the previous message instead |
| `split` | The same body, after the rewrite | Return several bodies, sent in order with `chunk_delay_ms` between them |
| `notice` | An accepted message, before the state owner queues an alert | Silence the sound, or add a toast in the window; the host also supplies the local hour and whether a game is running |
| `presence` | Startup and settings changes | Ask the owner to be set do not disturb while a game runs |
| `message_actions` / `run_action` | A message's own menu, then the entry the owner picked | Offer a clipboard or notice action on that message |
| `display` | Every frame, folded into one `Display` | Choose the clock format, an offset, relative rounding, the edited marker, the composer counter, and whether read state waits for you |

A queued reply keeps its delay and is sent by the app through its own send path, so permission
checks, the pending row and the service round trip behave exactly as for a typed message. A
reply for a channel that is not open is reported instead of silently switching conversations.

Ids resolve the way TestCord resolves them: exact id, then alias, then lowercase. That is what
lets an imported `settings.json` find a port by the name TestCord used.

## Bounds

Every hook is bounded by items and bytes, and an all-off registry costs no allocation on the
message path.

| Bound | Value |
|---|---|
| Plugins | 256, compiled in at build time |
| Settings per plugin | 48 keys, 16 KiB |
| Settings file | 256 KiB |
| Blocked keyword patterns | 256 patterns, 8 KiB total, 512 KiB regex size limit each |
| Auto-reply memory | 1024 processed ids, 256 tracked users and channels, 64 rate-window stamps |
| Queued replies | 8 messages, 8 KiB |
| Split message parts | 8 parts per message, 16 queued, 2000 characters each by default |
| Muted and exempt lists | 256 ids each |
| Clock offset | -720 to 840 minutes, a real time zone |
| Notification lists | 256 ids per list |
| Message-menu entries | 8 per message, 128 characters per label |
| Message log | 2000 entries, 4 MiB, 2000 characters per body, 256 KiB per export |

## Storage

`tesktop-plugins.json` beside the local database holds enable flags and settings only. A damaged
or foreign file is treated as absent so a bad edit can never keep the app from starting. The
MessageLogger record is session memory and is never written to disk; copy it out to keep it.

## Ported so far

| Plugin | What the port does | TestCord behaviour not ported |
|---|---|---|
| ClearURLs | Removes tracking parameters from links in outgoing bodies and edits | TestCord downloads the full rule database at startup; this port ships a bundled provider table for the widely used services, so uncommon providers are not covered |
| BlockKeywords | Ignores messages matching your words, in the body and in embed titles and descriptions | The second mode, which shows a matched message greyed out instead of dropping it |
| AutoReplyContent | Answers a trigger with one of your responses, with TestCord's channel, mention, cooldown and rate-limit rules | Responses are picked by a hash of the message id instead of `Math.random`, so a given message always gets the same answer |
| MessageLogger | Records created, edited and deleted messages, and copies the record out | The searchable history window, edit diffs and the deleted-message styling |
| SilenceUsers | Takes `@everyone`, role and user pings out of messages by the listed people | Dropping the desktop notification for those messages, which the state owner raises |
| SplitLargeMessages | Splits an oversized body on newlines, spaces or an exact length and sends the parts in order with your delay | Reading the account's Nitro tier for the 4000-character limit, and slowmode awareness |
| CustomTimestamps | Message clocks follow a 12 or 24 hour choice with the owner's own offset | The composer timestamp picker and its modal |
| DontRoundMyTimestamps | Relative phrases round down, so 7.6 years reads "7 years" | `moment`'s global rounding, which this client does not use |
| NoEditedTimestamp | Hides the `(edited)` marker | Nothing; the port is complete |
| PolishWording | Puts missing apostrophes back, expands contractions, capitalizes sentences and adds final periods, with a lowercase word list | The full rule set is TestCord's 43-entry contraction table, used as-is |
| ProfanityFilter | Removes filtered whole words from what you send, tidies the spaces and punctuation it leaves, and either sends a duck or refuses the message | The keyboard shortcut that toggles it |
| JsTextReplace | Applies your own find and replace rules, in order, each with an optional condition | The repeating rule editor widget: rules live in one multiline field, one per line, `find => replace` with an optional `\| if: text` |
| Signature | Appends your signature under every message you send | The composer button and composer menu entry that toggle it |
| EmbeddedURLs | Rewrites a recognised link to the form that embeds inline, leaving everything else alone | TestCord's per-origin map, which this port keeps short to the hosts it recognises |
| SentFromMyUname | Stamps a "Sent from my" line under what you send, with a per-channel whitelist and a `nouname ` one-message opt-out | Reading your uname: the text is yours to set |
| QuietHours | No alerts between the hours you pick, wrapping past midnight, with a toast still shown for the conversation you are reading | Nothing; the port is complete |
| AutoDNDWhilePlaying | Alerts are quiet while a game is running, with an opt-out and a do-not-disturb status intent | Writing the status to Discord, which the app's own presence owns |
| HideMessages | A hide entry in the message menu, and the message stays out of the conversation until the app restarts, bounded to 2048 ids | Hiding a direct message from the channel list |
| AntiDeleteMessage | Deleted bodies are kept so you can still read them, with the app's own recovery cache | The per-server exemption and the direct-message switch, because the app already protects direct messages itself |
| Annoiler | `/annoil` puts a spoiler around every character | Nothing; the port is complete |
| ClapText | `/clap` puts a clap between every word | Nothing; the port is complete |
| VibeCheck | `/vibe` drops a mood from TestCord's own list, chosen by the clock so the same second gives the same line | `Math.random` |
| BoldText | `/bold` turns your message into unicode bold, with TestCord's own code point maths | Nothing; the port is complete |
| LeetText | `/leet` converts your message with TestCord's swap table | Nothing; the port is complete |
| SmallCaps | `/smallcaps` turns your message into small caps | Nothing; the port is complete |
| VaporwaveText | `/vaporwave` turns your message into fullwidth characters | Nothing; the port is complete |
| WriteUpperCase | A capital at the start of every sentence you send, with an exception list, and closing punctuation that still ends a sentence | Nothing; the port is complete |
| FixCodeblockGap | A closing code fence always ends its line, so text below it is not glued on | Nothing; the port is complete |
| NormalizeMessageLinks | A canary or ptb link reads the way everyone else sees it | Nothing; the port is complete |
| MessageBurst | A second message inside the window edits the one before it, with attachment, reply and group-message rules | Its keyboard shortcut |
| PingNotifications | In servers, only ping on a direct mention, with friends and direct messages able to opt back in | The mention formatting it also rewrites, which this client renders itself |
| OnePingPerDM | A run of unread direct messages pings once, at the oldest, with scope, mention and ignore-list rules | The desktop-type check, which this client answers from the channel's guild instead |
| MessageNotifier | A toast for the listed people even where the app's own alert stays quiet | Nothing; the port is complete |
| CharacterCounter | A composer counter from the first character, coloured by percentage like TestCord | Nothing; the port is complete |
| StopAutoUnread | Messages stay unread while you read them, until you mark them yourself | Nothing; the port is complete |
| CopyUserURLs | Adds a Copy user link entry to a message's menu | The user context menu in the member list, which is a separate surface |
| CopyUserMention | Adds a Copy mention entry to a message's menu | As above |
| CopyStickerLinks | Adds a Copy sticker link entry to a message that carries stickers, with an option to copy an animated sticker as a still | The Open link entry and the sticker picker surface |
| NoReplyMention | Applies a reply-mention policy: never ping, ping only listed people, or leave your choice alone | Reading the Shift key: this client shows an explicit mention switch in the reply header, so the port applies a policy at send time instead |

## The rest of TestCord

`tools/generate-testcord-inventory.py <TestCord checkout>` regenerates
[testcord-inventory.csv](testcord-inventory.csv), which lists every plugin once with the hooks
it uses and a verdict:

| Verdict | Count | What it takes |
|---|---:|---|
| `portable-logic` | 229 | Settings and pure logic; the shape of the ports above |
| `portable-hook` | 75 | A hook on the message pipeline, plus whatever else it does |
| `portable-ui` | 76 | Declarative surfaces: commands, buttons, decorations, styles |
| `native-feature` | 67 | Ship a `native.ts`; here that is a Rust module doing the same work |
| `rewrite` | 284 | Patch Discord's own JavaScript; must be rebuilt against this client's state |
| `excluded` | 58 | Selfbot, token, mass-messaging, anti-logging or surveillance features |
| `excluded-web-only` | 1 | Only meaningful inside a patched web client |

The `ported` column fills in as the runtime grows, so the remaining work is a count rather than
a guess. The 284 rewrites are the honest ceiling on a native client: their entire premise is
rewriting Discord's minified internals, which this app never loads, so each one needs a design of
its own against the state owner rather than a translation.

## Tests

`cargo test -p tesktop-plugins` covers the registry, the bounds, settings persistence and
TestCord import, and each port's matching rules. `cargo test -p ui --lib` renders the settings
page. The host wiring in `apps/desktop/src/main.rs` is covered by `cargo check -p serein`; the
desktop unit tests do not currently build because of an unrelated upstream fixture error in
`apps/desktop/src/extension_member_details.rs`.
