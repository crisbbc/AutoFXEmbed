# Embedded-Link Transform Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Make AutoFxEmbed rewrite X/Twitter/Bluesky URLs that are embedded inside arbitrary clipboard text (e.g. `"blablabla https://x.com/user/status/123 blablabla"`), not just standalone single-link clips.

**Architecture:** Add a new `transform_text(text) -> Option<String>` entry point in `src/transform.rs` that first tries the existing single-link fast path (`transform_clipboard`, which trims and is strict), and on `None` falls back to a new `transform_embedded` scanner. The scanner splits the text on whitespace (preserving every byte of surrounding text/whitespace), offers each non-whitespace token to `transform_clipboard`, and reassembles. This reuses the proven host-matching/already-transformed-skip logic (DRY), is stdlib-only (no regex — keeps the size-optimized binary small), and naturally avoids the write-back loop because it returns `None` when nothing changed. The monitor swaps its one call site from `transform_clipboard` to `transform_text`.

**Tech Stack:** Rust 2021 edition, `windows-sys 0.61` (Win32 clipboard — unchanged by this plan), stdlib-only string scanning. Pure transform logic is unit-tested via integration tests in `tests/`; Win32 glue (monitor) is manual-only.

---

## Current context / assumptions

- `src/transform.rs` currently has one public fn `transform_clipboard` and a private `RULES` table + `host_matches` helper. `transform_clipboard` returns `Some` only for a whole trimmed single URL; prose → `None`.
- `host_matches(host, domain)` returns true iff `host == domain` or `host` ends with `.domain`. This already correctly skips already-transformed hosts: `fxtwitter.com` is not `twitter.com` and does not end with `.twitter.com`, so it is left alone. The embedded scanner reuses this for free.
- `src/monitor.rs:13` imports `transform_clipboard`; `src/monitor.rs:106` is the single call site `let Some(new_text) = transform_clipboard(&text) else { return; };`.
- `src/lib.rs` already declares `pub mod transform;` — no module wiring change needed; `transform_text` just needs to be `pub fn` in `src/transform.rs`.
- This is a **lib+bin** crate (`src/lib.rs` exists), so integration tests in `tests/*.rs` are run with `cargo test --test <file_stem>` (e.g. `cargo test --test transform`). Do NOT use `cargo test --lib` to run the `tests/` files.
- cargo/rustc live at `~/.cargo/bin` which is NOT on the default git-bash PATH. Prefix every cargo command: `export PATH="$HOME/.cargo/bin:$PATH"`.

## Proposed approach

Three tasks, each a separate commit, each leaving the crate building and tests green:

1. **Task 1 — Core `transform_text` + `transform_embedded`** (TDD, pure logic, integration-tested). The feature exists in the library but is not yet wired to the monitor.
2. **Task 2 — Widen the internal-whitespace rejection** in `transform_clipboard` from space/tab to *any* whitespace, so multi-URL text separated only by newlines falls through to the embedded scanner (which rewrites *every* URL, not just the first host). Edge-case robustness motivated by "links inside text" — text has newlines.
3. **Task 3 — Wire the monitor** to call `transform_text` instead of `transform_clipboard`. Feature goes live. Manual verification (Win32 glue has no automated tests).

## Files likely to change

- Modify: `src/transform.rs` — add `transform_text` + `transform_embedded` (Task 1); widen whitespace check (Task 2).
- Modify: `src/monitor.rs:13` and `src/monitor.rs:106` — swap `transform_clipboard` → `transform_text` (Task 3).
- Create: `tests/transform_text.rs` — new integration tests for `transform_text` (Tasks 1 and 2).
- Modify: `tests/transform.rs` — append one test for the newline-rejection fix (Task 2).
- Untouched: `src/lib.rs`, `src/main.rs`, `src/clipboard.rs`, `src/tray.rs`, `src/autostart.rs`, `Cargo.toml`, `build.rs`.

## Tests / validation

- `cargo test --test transform` — existing 18 tests must stay green (no behavior change to `transform_clipboard` except the Task 2 newline case, which has no existing test).
- `cargo test --test transform_text` — new tests for `transform_text`.
- `cargo test` — full suite (lib unit + both integration files) must pass.
- `cargo build` — confirm the monitor wiring compiles.
- Manual: run the binary, copy `blablabla https://x.com/user/status/123 blablabla`, paste, confirm it becomes `blablabla https://fixupx.com/user/status/123 blablabla`. Then copy a bare `https://x.com/user/status/123` and confirm it still trims/transforms. Then copy `https://fxtwitter.com/foo` and confirm no loop (clipboard unchanged).

---

### Task 1: Add `transform_text` + `transform_embedded` (core embedded rewrite)

**Objective:** Add a new public `transform_text` that transforms URLs embedded in prose, reusing `transform_clipboard` per token, while preserving all surrounding text and whitespace.

**Files:**
- Modify: `src/transform.rs` (append after the existing `transform_clipboard` fn, before EOF)
- Create: `tests/transform_text.rs`

**Step 1: Write failing tests**

Create `tests/transform_text.rs`:

```rust
use autofxembed::transform::transform_text;

#[test]
fn transform_text_embeds_twitter_in_prose() {
    assert_eq!(
        transform_text("check out https://twitter.com/foo"),
        Some("check out https://fxtwitter.com/foo".to_string())
    );
}

#[test]
fn transform_text_embeds_x_in_prose() {
    assert_eq!(
        transform_text("see https://x.com/a/status/1 here"),
        Some("see https://fixupx.com/a/status/1 here".to_string())
    );
}

#[test]
fn transform_text_embeds_bsky_in_prose() {
    assert_eq!(
        transform_text("look https://bsky.app/profile/u.bsky.social yeah"),
        Some("look https://fxbsky.app/profile/u.bsky.social yeah".to_string())
    );
}

#[test]
fn transform_text_embeds_schemeless_url() {
    assert_eq!(
        transform_text("see twitter.com/foo here"),
        Some("see fxtwitter.com/foo here".to_string())
    );
}

#[test]
fn transform_text_embeds_multiple_urls() {
    assert_eq!(
        transform_text("a https://twitter.com/1 b https://x.com/2 c"),
        Some("a https://fxtwitter.com/1 b https://fixupx.com/2 c".to_string())
    );
}

#[test]
fn transform_text_single_link_delegates_and_trims() {
    // Fast path: a lone URL is handled by transform_clipboard, which trims.
    assert_eq!(
        transform_text("  https://twitter.com/foo\n"),
        Some("https://fxtwitter.com/foo".to_string())
    );
}

#[test]
fn transform_text_preserves_surrounding_whitespace_in_prose() {
    // Embedded path keeps surrounding whitespace verbatim (does not trim).
    assert_eq!(
        transform_text("  hi https://twitter.com/foo  "),
        Some("  hi https://fxtwitter.com/foo  ".to_string())
    );
}

#[test]
fn transform_text_preserves_newlines_between_tokens() {
    assert_eq!(
        transform_text("line1\nhttps://twitter.com/foo\nline3"),
        Some("line1\nhttps://fxtwitter.com/foo\nline3".to_string())
    );
}

#[test]
fn transform_text_skips_already_transformed_embedded() {
    assert_eq!(transform_text("see https://fxtwitter.com/foo"), None);
}

#[test]
fn transform_text_skips_plain_text() {
    assert_eq!(transform_text("Hello world"), None);
}

#[test]
fn transform_text_skips_non_supported_url_in_prose() {
    assert_eq!(transform_text("see https://example.com/foo"), None);
}

#[test]
fn transform_text_skips_empty_and_whitespace_only() {
    assert_eq!(transform_text(""), None);
    assert_eq!(transform_text("   "), None);
    assert_eq!(transform_text("\n\t  \n"), None);
}
```

**Step 2: Run tests to verify failure**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --test transform_text
```
Expected: FAIL — `error[E0425]: cannot find function transform_text in module autofxembed::transform` (function does not exist yet), so every test fails to compile.

**Step 3: Write minimal implementation**

Append to `src/transform.rs` (after the closing `}` of `transform_clipboard`, at EOF):

```rust
/// If `text` is a single supported URL, OR contains one or more supported URLs
/// embedded in surrounding text, return `text` with every such URL rewritten to
/// its FxEmbed form (surrounding text and whitespace preserved). Otherwise
/// return `None` (leave the clipboard untouched). Never rewrites an
/// already-transformed host.
///
/// Single-link inputs are trimmed and handled by [`transform_clipboard`]; prose
/// with embedded links preserves all surrounding text verbatim.
pub fn transform_text(text: &str) -> Option<String> {
    // Fast path: the whole input is one clean URL (trims surrounding ws).
    if let Some(out) = transform_clipboard(text) {
        return Some(out);
    }
    transform_embedded(text)
}

/// Scan `text` for URLs embedded in prose and rewrite each one in place.
/// Whitespace (spaces, tabs, newlines, …) splits tokens and is preserved
/// verbatim; each non-whitespace token is offered to [`transform_clipboard`].
/// Returns `None` when no token changed (so the clipboard is left alone and we
/// avoid a re-write loop).
fn transform_embedded(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len() + 8);
    let mut changed = false;
    let mut rest = text;

    while !rest.is_empty() {
        // Leading whitespace run: copy it verbatim.
        let after_ws = rest.trim_start_matches(|c: char| c.is_whitespace());
        let ws_len = rest.len() - after_ws.len();
        out.push_str(&rest[..ws_len]);
        rest = after_ws;
        if rest.is_empty() {
            break;
        }
        // Next token: everything up to the next whitespace char (or end).
        let tok_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        let token = &rest[..tok_end];
        match transform_clipboard(token) {
            Some(rewritten) => {
                out.push_str(&rewritten);
                changed = true;
            }
            None => out.push_str(token),
        }
        rest = &rest[tok_end..];
    }

    if changed { Some(out) } else { None }
}
```

Notes for the implementer:
- `trim_start_matches(|c: char| c.is_whitespace())` and `find(|c: char| c.is_whitespace())` operate on char boundaries, so every index derived from them (`ws_len`, `tok_end`) is a valid UTF-8 boundary and the slices are sound for non-ASCII prose (accented text, CJK, emoji).
- A token has no internal whitespace by construction, so `transform_clipboard`'s internal-whitespace check is a no-op per token; the per-token call reduces to scheme + host_match + rewrite.
- `transform_clipboard` already skips already-transformed hosts via `host_matches`, so embedded `fxtwitter.com`/`fixupx.com`/`fxbsky.app` tokens are left alone and `changed` stays false for them.

**Step 4: Run tests to verify pass**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --test transform_text
cargo test --test transform
```
Expected: `cargo test --test transform_text` → 12 passed. `cargo test --test transform` → 18 passed (unchanged).

**Step 5: Commit**

```bash
git add src/transform.rs tests/transform_text.rs
git commit -m "feat: rewrite X/Twitter/Bluesky URLs embedded in clipboard text"
```

---

### Task 2: Widen internal-whitespace rejection to any whitespace

**Objective:** Fix a latent gap so multi-URL text separated only by newlines (no spaces) is handled by the embedded scanner (which rewrites *every* URL) instead of being mis-transformed by the single-link fast path (which only rewrites the first host). The current check `contains(' ') || contains('\t')` lets newlines through.

**Files:**
- Modify: `src/transform.rs:30-33` (the internal-whitespace guard inside `transform_clipboard`)
- Modify: `tests/transform.rs` (append one test)
- Modify: `tests/transform_text.rs` (append one test)

**Step 1: Write failing tests**

Append to `tests/transform.rs` (after the existing `skips_prose_containing_url` test at EOF):

```rust
#[test]
fn skips_url_with_internal_newline() {
    // A newline inside what looks like a single URL means it isn't one clean
    // URL — leave it for the embedded-link path to handle token by token.
    assert_eq!(
        transform_clipboard("https://twitter.com/a\nhttps://x.com/b"),
        None
    );
}
```

Append to `tests/transform_text.rs` (at EOF):

```rust
#[test]
fn transform_text_transforms_each_newline_separated_url() {
    // Two tweet links, one per line, no spaces: every URL must be rewritten
    // (the fast path must NOT swallow this into a single-host rewrite).
    assert_eq!(
        transform_text("https://twitter.com/a\nhttps://x.com/b"),
        Some("https://fxtwitter.com/a\nhttps://fixupx.com/b".to_string())
    );
}
```

**Step 2: Run tests to verify failure**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --test transform skips_url_with_internal_newline
cargo test --test transform_text transform_text_transforms_each_newline_separated_url
```
Expected: both FAIL.
- `skips_url_with_internal_newline`: pre-fix `transform_clipboard("https://twitter.com/a\nhttps://x.com/b")` returns `Some("https://fxtwitter.com/a\nhttps://x.com/b")` (only the first host rewritten, the `\n` and the second URL are swallowed into the path because newline is not rejected) — the assertion of `None` fails.
- `transform_text_transforms_each_newline_separated_url`: pre-fix the fast path returns the partial `Some("https://fxtwitter.com/a\nhttps://x.com/b")` (x.com NOT rewritten), so the assertion expecting `fixupx.com` fails.

**Step 3: Write minimal implementation**

In `src/transform.rs`, replace the internal-whitespace guard (currently):

```rust
    // Only rewrite clean single URLs (no internal whitespace).
    if after_scheme.contains(' ') || after_scheme.contains('\t') {
        return None;
    }
```

with:

```rust
    // Only rewrite clean single URLs (no internal whitespace of any kind —
    // spaces, tabs, newlines, …). Anything with internal whitespace is left
    // for `transform_embedded` to handle token by token.
    if after_scheme.chars().any(|c| c.is_whitespace()) {
        return None;
    }
```

**Step 4: Run tests to verify pass**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --test transform
cargo test --test transform_text
```
Expected: `cargo test --test transform` → 19 passed (the original 18 + `skips_url_with_internal_newline`). `cargo test --test transform_text` → 13 passed (the 12 from Task 1 + the newline-separated case). Confirm none of the original 18 regressed (in particular `trims_surrounding_whitespace` still passes — the trailing `\n` there is removed by `trim()` before the scheme split, so the guard is never reached for that input).

**Step 5: Commit**

```bash
git add src/transform.rs tests/transform.rs tests/transform_text.rs
git commit -m "fix: reject any internal whitespace so newline-separated URLs all get rewritten"
```

---

### Task 3: Wire the monitor to `transform_text`

**Objective:** Make the live clipboard monitor use the new `transform_text` so embedded links are actually rewritten at runtime.

**Files:**
- Modify: `src/monitor.rs:13` (import) and `src/monitor.rs:106` (call site)

**Step 1: (no automated test — Win32 glue is manual-only)**

There is no unit test for the monitor (it drives Win32 message-loop glue that requires a real window/session). The change is verified by `cargo build` (compiles) plus the manual check in Step 4. The transform behavior itself is already covered by the Task 1 + Task 2 integration tests.

**Step 2: (n/a — nothing to fail first)**

**Step 3: Write the change**

In `src/monitor.rs`, change the import (line 13):

```rust
use crate::transform::transform_clipboard;
```
→
```rust
use crate::transform::transform_text;
```

And change the call site inside `handle_clipboard_update` (line 106):

```rust
    let Some(new_text) = transform_clipboard(&text) else {
```
→
```rust
    let Some(new_text) = transform_text(&text) else {
```

No other change. The deferred-write mechanism (`PENDING_WRITE`, `WM_DO_WRITE`, `do_pending_write`) is unchanged: `transform_text` returns `None` when nothing changed (including for already-transformed text), so the existing loop-avoidance still holds.

**Step 4: Verify build + manual run**

Run:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build
cargo test
```
Expected: `cargo build` → Finished. `cargo test` → all pass (transform: 19, transform_text: 13, plus any lib unit tests).

Manual verification (run the app, then test each clip):
1. Copy `blablabla https://x.com/user/status/123 blablabla` → paste → expect `blablabla https://fixupx.com/user/status/123 blablabla`.
2. Copy a bare `https://twitter.com/user/status/123` (with a trailing newline if your browser adds one) → paste → expect `https://fxtwitter.com/user/status/123` (trimmed).
3. Copy `https://fxtwitter.com/foo` → paste → expect unchanged (no rewrite loop).
4. Copy `see https://twitter.com/a and https://x.com/b ok` → paste → expect `see https://fxtwitter.com/a and https://fixupx.com/b ok`.

**Step 5: Commit**

```bash
git add src/monitor.rs
git commit -m "feat: monitor rewrites embedded links via transform_text"
```

---

## Risks, tradeoffs, and open questions

- **Trailing punctuation attached to a URL** (e.g. `"see https://twitter.com/foo."`): the token is `https://twitter.com/foo.` and `transform_clipboard` treats the trailing `.` as part of the path, yielding `https://fxtwitter.com/foo.`. This is the same behavior as a lone link and is **accepted**: separating the period from the URL would require *inserting* a space (altering the user's text), which we will not do. (Moving the stripped punctuation into the following gap is a cosmetic no-op — the period stays adjacent because there is no whitespace between the URL and the punctuation.) If FxEmbed ever fails to resolve URLs with trailing sentence punctuation, the only sound fix is to drop those trailing bytes, which would lose the user's punctuation — deliberately out of scope. Documented here so it is a conscious decision, not a surprise.
- **Surrounding whitespace is preserved for prose but trimmed for lone links.** Intentional: prose formatting should be preserved verbatim; a stray trailing newline on a lone copied link should not (it can auto-send in chat apps). The fast path (`transform_clipboard`) trims; the embedded path does not. Documented in the `transform_text` doc comment.
- **No new dependencies / no regex.** Keeps the size-optimized release binary (`opt-level=z`, `lto`, `strip`) small, consistent with the project ethos. Scanning is O(n) with one output `String` allocation — trivial for clipboard-sized text.
- **Loop safety.** `transform_embedded` returns `None` when no token changed. Already-transformed hosts are skipped by `host_matches` (e.g. `fxtwitter.com` is not `twitter.com` and does not end with `.twitter.com`), so re-reading the clipboard after a write yields `None` and the monitor does not write again. Covered by `transform_text_skips_already_transformed_embedded` and the manual no-loop check.
- **Monitor change is untested by automation.** The Win32 message-loop glue has no unit tests by design (manual-only). The one-line swap is verified by `cargo build` + the four manual clip checks in Task 3 Step 4. The actual transform behavior is fully covered by the `tests/transform_text.rs` and `tests/transform.rs` integration tests.
- **Open question (not in scope):** Should the tray menu or a future settings UI expose a toggle between "single-link only" and "embedded" modes? Currently the new behavior is unconditional. Defer until a user asks.
