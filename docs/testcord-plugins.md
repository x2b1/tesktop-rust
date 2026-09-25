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
| `split` | The same body, after the rewrite | Return several bodies, sent in order with `chunk_delay_ms` between them |

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
| Plugins | 16, fixed at build time |
| Settings per plugin | 48 keys, 16 KiB |
| Settings file | 256 KiB |
| Blocked keyword patterns | 256 patterns, 8 KiB total, 512 KiB regex size limit each |
| Auto-reply memory | 1024 processed ids, 256 tracked users and channels, 64 rate-window stamps |
| Queued replies | 8 messages, 8 KiB |
| Split message parts | 8 parts per message, 16 queued, 2000 characters each by default |
| Muted and exempt lists | 256 ids each |
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
