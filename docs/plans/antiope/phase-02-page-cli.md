# Phase 2: `ressac page list` and `ressac page set`

**Runs after:** phase 6 (which runs right after phase 1b).

Read the plan index first. Global Constraints apply. This phase implements the CLI half of spec §4 ("CLI"), the `page set invalid` row of §10, and the CLI items of §11 (`page set` exit `2` for each refusal, the `page list --json` shape). None of the plan index's five Review Focus items belongs to this phase.

**Changes from the plan index** (see Phase 1's header for the reasons):
- **The v3 contract fixture and the schema drift test are done in Phase 1**, which bumps the schema and so must regenerate the fixture to stay green (1a at v3, 1b with page numbers). This phase changes no schema and does not touch `contract.rs` or the fixture.
- **`Ressac::pages` and `Ressac::set_page` go just before `Ressac::summary`.** `Ressac` has no garden methods: the garden operations are free functions in `garden.rs` taking `&Store`.

**Files (4):**
- Modify: `crates/ressac/src/main.rs` (Step 0 cleanup, the `page` subcommand, tests inline)
- Modify: `crates/ressac-core/src/service.rs` (`Event::Page`, `Ressac::pages`, `Ressac::set_page`, tests inline)
- Modify: `crates/ressac/src/output.rs` (`page_text`, the sink's new event arm, tests inline)
- Modify: `crates/ressac/tests/cli.rs`

**Builds on (Phase 1, exact):**
- `ressac_core::store::pages::{PageKind, PageRow, list, set}`; `PageKind::as_str(self) -> &'static str`;
- `pages::list(conn) -> Result<Vec<PageRow>>`: 8 reserved rows, then `v_page` by page;
- `pages::set(conn, kind, id, page) -> Result<()>`, refusing with `Error::PageReserved(u16)` (`Page N is reserved.`), `Error::PageOutOfRange(u16)` (`Page N is not between 110 and 799.`), `Error::PageTaken { page }` (`Page N is already taken.`) or `Error::UnknownPageTarget { kind, id }` (`No feed #N.` / `No folder #N.`);
- `Store::import_outlines` numbers new folders and feeds. For `tests/fixtures/vienna-like.opml` on a fresh database that gives folder ids Blogs 1, potes 2, Biblis 3, Infos 4, Science 5, Jon 6, YouTube 7, feed ids 1 to 11 in file order (Daring Fireball (1) is merged away), and the pages listed in the CLI test below.

**Existing CLI machinery used as is:** `UsageError` / `usage(…)` (downcast in `main`, exit `2`, bare message on stderr, nothing on stdout); `open_with_feeds(db)` (first-run hint and exit `0` on a missing or empty database); `output::write_result(out, json, &Event, text)`; the `--json` and `--db` global flags; the test helpers `ressac`, `stdout`, `stderr`, `json_lines` and `FIXTURE` in `crates/ressac/tests/cli.rs`.

**CLI contract produced by this phase** (`ressac [--db P] [--json] page …`):

| Command | `--json` stdout | Text stdout | Exit `2` (message on stderr, nothing on stdout) |
|---|---|---|---|
| `page list` | one `page` event per row, reserved pages first | one `page kind id title` line per row, `-` for a reserved page's id | none |
| `page set feed\|folder ID PAGE` | one `page` event: the renumbered row, re-read from `v_page` | that row's line | `Page N is reserved.`, `Page N is not between 110 and 799.`, `Page N is already taken.`, `No feed #N.`, `No folder #N.`; clap's own for `page` alone, a kind other than `feed`/`folder`, or a page that is not a `u16` |

On a database with no feeds, both subcommands print the existing first-run hint and exit `0`, like `garden`. A database error exits `1` with `error: …`.

**Event shape:** `{"event":"page","page":412,"kind":"feed","id":12,"title":"Courrier international","parent_page":400}`; a reserved row has `"kind":"reserved","id":null,"parent_page":null`.

---

### Task 2.0: Step 0, dead-code pass on `main.rs`

`crates/ressac/src/main.rs` has 538 lines and this phase restructures its command set, so the Step 0 rule applies.

**Files:**
- Modify: `crates/ressac/src/main.rs`

**Interfaces:** none change.

- [ ] **Step 1: Audit.**

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
wc -l crates/ressac/src/main.rs
grep -nE 'dbg!|todo!|unimplemented!|println!\("DEBUG|#\[allow\(dead_code\)\]|#\[allow\(unused' crates/ressac/src/main.rs
grep -n 'ressac_core::garden::' crates/ressac/src/main.rs
```

Expected: `538 crates/ressac/src/main.rs`; the first `grep` prints nothing (no debug output, no silenced lints; rustc's `dead_code` and `unused_imports`, denied by the clippy gate, already rule out unused functions and imports in a binary crate); the second prints one line, `460:            ressac_core::garden::recompute_triage(app.store())?;`, a fully qualified path although `use ressac_core::garden;` is already at the top of the file.

- [ ] **Step 2: Clean.** In `Command::Migrate`, replace that line with:

```rust
            garden::recompute_triage(app.store())?;
```

- [ ] **Step 3: Verify.** `cargo clippy -p ressac --all-targets -- -D warnings` → no warning; `cargo test -p ressac` → `31 passed` (unit) and `13 passed` (`cli`).

- [ ] **Step 4: Commit.**

```bash
git add crates/ressac/src/main.rs
git commit -m "main.rs: use the imported garden module in migrate (Step 0 cleanup)" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2.1: `Event::Page`, `Ressac::pages`, `Ressac::set_page`

**Files:**
- Modify: `crates/ressac-core/src/service.rs`
- Modify: `crates/ressac/src/output.rs` (the progress sink's `match` must name the new variant, or the binary stops compiling)

**Interfaces:**
- Consumes: `pages::{list, set}`, `PageKind`, `PageRow` (Phase 1); `Store::conn` (`pub(crate)`, reachable from `service.rs`).
- Produces:
  - `Event::Page(PageRow)`, a newtype variant of the internally tagged `Event`: serde writes `"event":"page"` and then `PageRow`'s fields in declaration order;
  - `pub fn pages(&self) -> Result<Vec<PageRow>>` and `pub fn set_page(&self, kind: PageKind, id: i64, page: u16) -> Result<()>` on `Ressac`.

- [ ] **Step 1: Write the failing tests.** In `crates/ressac-core/src/service.rs`, `mod tests`, insert right before `const PARA: &str = …`:

```rust
    #[test]
    fn page_events_have_stable_json() {
        let row = PageRow {
            page: 412,
            kind: PageKind::Feed,
            id: Some(12),
            title: "Courrier international".into(),
            parent_page: Some(400),
        };
        assert_eq!(
            serde_json::to_string(&Event::Page(row)).unwrap(),
            r#"{"event":"page","page":412,"kind":"feed","id":12,"title":"Courrier international","parent_page":400}"#
        );
        let reserved = PageRow {
            page: 899,
            kind: PageKind::Reserved,
            id: None,
            title: "Guide".into(),
            parent_page: None,
        };
        assert_eq!(
            serde_json::to_value(Event::Page(reserved)).unwrap(),
            serde_json::json!({"event": "page", "page": 899, "kind": "reserved", "id": null, "title": "Guide", "parent_page": null})
        );
    }

    #[test]
    fn pages_and_set_page_go_through_the_store() {
        let app = Ressac::with_parts(
            Store::open_in_memory().unwrap(),
            FetchLimits::default(),
            "http://127.0.0.1:9",
        )
        .unwrap();
        app.import_opml(
            r#"<opml><body><outline text="Infos"><outline text="A" xmlUrl="https://a.example.org/feed"/></outline></body></opml>"#,
            false,
        )
        .unwrap();
        let numbered = |app: &Ressac| -> Vec<(u16, PageKind, Option<i64>)> {
            app.pages()
                .unwrap()
                .into_iter()
                .skip(8)
                .map(|r| (r.page, r.kind, r.id))
                .collect()
        };
        assert_eq!(
            numbered(&app),
            [
                (200, PageKind::Folder, Some(1)),
                (201, PageKind::Feed, Some(1))
            ]
        );
        app.set_page(PageKind::Feed, 1, 412).unwrap();
        assert_eq!(numbered(&app)[1], (412, PageKind::Feed, Some(1)));
        assert!(matches!(
            app.set_page(PageKind::Feed, 1, 200),
            Err(crate::Error::PageTaken { page: 200 })
        ));
    }
```

- [ ] **Step 2: Run them.** `cargo test -p ressac-core --lib service::tests::page`. Expected: compile errors, among them ``cannot find struct, variant or union type `PageRow` in this scope``, ``cannot find type `PageKind` in this scope``, ``no variant, associated function, or constant named `Page` found for enum `service::Event` `` and ``no method named `set_page` found for struct `service::Ressac` ``.

- [ ] **Step 3: Implement.**

In `service.rs`, add the import above `use crate::store::{FeedRow, ImportReport, Store};`:

```rust
use crate::store::pages::{self, PageKind, PageRow};
```

add the variant at the end of `enum Event`, after `Undone { … },`:

```rust
    /// One line of `ressac page list`, or the row `ressac page set` renumbered.
    Page(PageRow),
```

and the two methods in `impl Ressac`, right before `/// Status counts and safe repairs over active feeds…` (`pub fn summary`):

```rust
    /// Every teletext page: the reserved ones, then each numbered folder and feed.
    pub fn pages(&self) -> Result<Vec<PageRow>> {
        pages::list(self.store.conn())
    }

    /// Gives a folder or a feed another page; see `store::pages::set` for the refusals.
    pub fn set_page(&self, kind: PageKind, id: i64, page: u16) -> Result<()> {
        pages::set(self.store.conn(), kind, id, page)
    }
```

In `crates/ressac/src/output.rs`, `impl EventSink for Sink`, the progress arm ignores the one-shot events; add the new one there:

```rust
                Event::WaybackDone { .. }
                | Event::Finished { .. }
                | Event::Extracted { .. }
                | Event::Decided { .. }
                | Event::Undone { .. }
                | Event::Page(_) => {}
```

- [ ] **Step 4: Run.** `cargo test -p ressac-core --lib` → `250 passed; 0 failed; 1 ignored`; `cargo test -p ressac` → `31 passed` and `13 passed`.

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add crates/ressac-core/src/service.rs crates/ressac/src/output.rs
git commit -m "Facade for page numbers: Ressac::pages, Ressac::set_page and the page event" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Task 2.2: `ressac page list` and `ressac page set`

**Files:**
- Modify: `crates/ressac/src/output.rs`
- Modify: `crates/ressac/src/main.rs`
- Modify: `crates/ressac/tests/cli.rs`

**Interfaces:**
- Consumes: `Ressac::{pages, set_page}`, `Event::Page` (Task 2.1); `PageKind::as_str`; the four `Error` refusals (Phase 1).
- Produces (binary crate):
  - `pub fn page_text(row: &PageRow) -> String` in `output.rs`, giving `412 feed 12 Courrier international` or `100 reserved - Index`;
  - `Command::Page { action: PageCommand }`, with `PageCommand::List` and `PageCommand::Set { kind: PageKindArg, id: i64, page: u16 }`, and `PageKindArg::{Feed, Folder}` → `PageKind`;
  - private `fn page_error(e: ressac_core::Error) -> anyhow::Error`, which turns the four refusals into usage errors (exit `2`) and keeps every other error fatal (exit `1`);
  - private `fn page_command(app: &Ressac, action: PageCommand, json: bool, out: &mut impl Write) -> anyhow::Result<()>`.

- [ ] **Step 1: Write the failing tests.**

In `crates/ressac/src/output.rs`, append at the end of `mod tests`:

```rust
    #[test]
    fn page_text_is_page_kind_id_title() {
        use ressac_core::store::pages::PageKind;
        let feed = PageRow {
            page: 412,
            kind: PageKind::Feed,
            id: Some(12),
            title: "Courrier international".into(),
            parent_page: Some(400),
        };
        assert_eq!(page_text(&feed), "412 feed 12 Courrier international");
        let index = PageRow {
            page: 100,
            kind: PageKind::Reserved,
            id: None,
            title: "Index".into(),
            parent_page: None,
        };
        assert_eq!(page_text(&index), "100 reserved - Index");
    }
```

In `crates/ressac/src/main.rs`, append at the end of `mod tests`:

```rust
    #[test]
    fn page_set_parses_its_kind_id_and_page() {
        let set = Cli::try_parse_from(["ressac", "page", "set", "folder", "4", "450"]).unwrap();
        assert!(matches!(
            set.command,
            Command::Page {
                action: PageCommand::Set {
                    kind: PageKindArg::Folder,
                    id: 4,
                    page: 450
                }
            }
        ));
        for bad in [
            &["ressac", "page"][..],
            &["ressac", "page", "set", "entry", "1", "300"],
            &["ressac", "page", "set", "feed", "1", "70000"],
            &["ressac", "page", "set", "feed", "1", "-3"],
        ] {
            assert!(Cli::try_parse_from(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn page_refusals_are_usage_errors_and_others_stay_fatal() {
        use ressac_core::Error;
        for refusal in [
            Error::PageReserved(800),
            Error::PageOutOfRange(900),
            Error::PageTaken { page: 201 },
            Error::UnknownPageTarget {
                kind: "feed",
                id: 99,
            },
        ] {
            let text = refusal.to_string();
            let e = page_error(refusal);
            assert_eq!(
                e.downcast_ref::<UsageError>().map(ToString::to_string),
                Some(text)
            );
        }
        let fatal = page_error(Error::Invalid("disk".into()));
        assert!(fatal.downcast_ref::<UsageError>().is_none());
    }
```

In `crates/ressac/tests/cli.rs`, add two cases at the end of the list in `commands_on_an_empty_store_hint_at_import`:

```rust
        &["garden", "apply-safe"],
        &["page", "list"],
        &["page", "set", "feed", "1", "300"],
    ] {
```

three at the end of the list in `usage_errors_exit_2`:

```rust
        &["fetch", "--feed", "1", "--folder", "1"],
        &["page"],
        &["page", "set", "entry", "1", "300"],
        &["page", "set", "feed", "1", "70000"],
    ] {
```

and append at the end of the file:

```rust
#[test]
fn page_list_prints_reserved_pages_then_every_folder_and_feed() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let text = ressac(home.path(), &["page", "list"]);
    assert!(text.status.success(), "{text:?}");
    assert_eq!(
        stdout(&text),
        "100 reserved - Index\n\
         101 reserved - Unread\n\
         102 reserved - Today\n\
         103 reserved - Starred\n\
         800 reserved - Needs triage\n\
         801 reserved - All feeds\n\
         802 reserved - Cemetery\n\
         899 reserved - Guide\n\
         200 folder 1 Blogs\n\
         201 feed 1 xkcd.com\n\
         202 folder 2 potes\n\
         203 feed 2 Orange Drenka\n\
         204 feed 3 Orange-Drenka\n\
         205 feed 4 Photos from bouquiniste\n\
         206 feed 5 Girl on the Couch\n\
         300 folder 3 Biblis\n\
         301 feed 6 Bibliobsession 2.0\n\
         302 feed 7 Unphotographable\n\
         400 folder 4 Infos\n\
         401 feed 8 Daring Fireball\n\
         402 feed 9 Six Colors\n\
         403 folder 5 Science\n\
         404 feed 10 Center for Humane Technology\n\
         500 folder 6 Jon\n\
         501 folder 7 YouTube\n\
         502 feed 11 Center for Humane Technology (1)\n"
    );

    let json = ressac(home.path(), &["--json", "page", "list"]);
    assert!(json.status.success(), "{json:?}");
    let lines = json_lines(&json);
    assert_eq!(lines.len(), 26);
    assert_eq!(
        lines[0],
        json!({"event": "page", "page": 100, "kind": "reserved", "id": null, "title": "Index", "parent_page": null})
    );
    assert_eq!(
        lines[11],
        json!({"event": "page", "page": 203, "kind": "feed", "id": 2, "title": "Orange Drenka", "parent_page": 202})
    );
    assert!(lines.iter().all(|l| l["event"] == "page"));
}

#[test]
fn page_set_renumbers_and_prints_the_row() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let run = |args: &[&str]| ressac(home.path(), args);

    let feed = run(&["page", "set", "feed", "2", "412"]);
    assert!(feed.status.success(), "{feed:?}");
    assert_eq!(stdout(&feed), "412 feed 2 Orange Drenka\n");
    let folder = run(&["--json", "page", "set", "folder", "4", "450"]);
    assert!(folder.status.success(), "{folder:?}");
    assert_eq!(
        json_lines(&folder),
        [
            json!({"event": "page", "page": 450, "kind": "folder", "id": 4, "title": "Infos", "parent_page": null})
        ]
    );
    let list = stdout(&run(&["page", "list"]));
    assert!(list.contains("412 feed 2 Orange Drenka\n"), "{list}");
    assert!(!list.contains("203 feed 2"), "{list}");
    let infos = json_lines(&run(&["--json", "page", "list"]));
    let df = infos
        .iter()
        .find(|l| l["title"] == "Daring Fireball")
        .unwrap();
    assert_eq!(
        df["parent_page"], 450,
        "children follow their folder's new page"
    );
}

#[test]
fn page_set_refusals_exit_2_and_write_nothing() {
    let home = tempfile::tempdir().unwrap();
    assert!(ressac(home.path(), &["import", FIXTURE]).status.success());
    let run = |args: &[&str]| ressac(home.path(), args);
    for (args, message) in [
        (
            &["page", "set", "feed", "2", "800"][..],
            "Page 800 is reserved.\n",
        ),
        (
            &["page", "set", "feed", "2", "100"],
            "Page 100 is reserved.\n",
        ),
        (
            &["page", "set", "feed", "2", "900"],
            "Page 900 is not between 110 and 799.\n",
        ),
        (
            &["page", "set", "feed", "2", "109"],
            "Page 109 is not between 110 and 799.\n",
        ),
        (
            &["page", "set", "feed", "2", "201"],
            "Page 201 is already taken.\n",
        ),
        (
            &["page", "set", "feed", "2", "200"],
            "Page 200 is already taken.\n",
        ),
        (
            &["page", "set", "folder", "1", "201"],
            "Page 201 is already taken.\n",
        ),
        (&["page", "set", "feed", "99", "300"], "No feed #99.\n"),
        (&["page", "set", "folder", "99", "300"], "No folder #99.\n"),
        (
            &["--json", "page", "set", "feed", "99", "300"],
            "No feed #99.\n",
        ),
    ] {
        let out = run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {out:?}");
        assert_eq!(stderr(&out), message, "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
    }
    let list = stdout(&run(&["page", "list"]));
    assert!(
        list.contains("203 feed 2 Orange Drenka\n"),
        "unchanged: {list}"
    );
}

#[test]
fn page_commands_exit_1_on_a_database_error() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("db.sqlite"), b"this is not a database").unwrap();
    for args in [&["page", "list"][..], &["page", "set", "feed", "1", "300"]] {
        let out = ressac(home.path(), args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
        assert!(
            stderr(&out).starts_with("error: cannot open database"),
            "{out:?}"
        );
    }
}
```

- [ ] **Step 2: Run them.** `cargo test -p ressac`. Expected: the unit tests do not compile: ``cannot find function `page_text` in this scope``, ``cannot find function `page_error` in this scope``, ``cannot find type `PageCommand` in this scope``, ``cannot find type `PageKindArg` in this scope``, ``no variant named `Page` found for enum `Command` `` and ``cannot find struct, variant or union type `PageRow` in this scope``. (`cargo test -p ressac --test cli` alone compiles and fails 5 tests: the binary has no `page` subcommand yet, so it exits `2` where they expect `0` or `1`, or prints clap's error instead of the refusal. `usage_errors_exit_2` already passes, since an unknown subcommand exits `2` too.)

- [ ] **Step 3: Implement.**

In `crates/ressac/src/output.rs`, add the import under `use ressac_core::store::ImportReport;`:

```rust
use ressac_core::store::pages::PageRow;
```

and the function right before `/// Printed on stderr after a run stopped by Ctrl-C.`:

```rust
/// One line of `ressac page list` and `ressac page set`: `page kind id title`, with `-`
/// for the id of a reserved page.
pub fn page_text(row: &PageRow) -> String {
    let id = row.id.map_or("-".to_string(), |id| id.to_string());
    format!("{} {} {id} {}", row.page, row.kind.as_str(), row.title)
}
```

In `crates/ressac/src/main.rs`:

1. Add the import under `use ressac_core::store::Store;`:

```rust
use ressac_core::store::pages::PageKind;
```

2. Add the variant at the end of `enum Command`, after `Migrate,`:

```rust
    /// List the teletext page numbers, or give a folder or a feed another one
    Page {
        #[command(subcommand)]
        action: PageCommand,
    },
```

3. Add the subcommand types right before `impl From<DecisionArg> for Decision {`:

```rust
#[derive(Subcommand)]
enum PageCommand {
    /// Every page, reserved ones first: page, kind, id, title
    List,
    /// Give a folder or a feed another page (110 to 799, not reserved, not taken)
    Set {
        #[arg(value_enum)]
        kind: PageKindArg,
        /// The folder's or the feed's id
        id: i64,
        page: u16,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum PageKindArg {
    Feed,
    Folder,
}

impl From<PageKindArg> for PageKind {
    fn from(k: PageKindArg) -> Self {
        match k {
            PageKindArg::Feed => PageKind::Feed,
            PageKindArg::Folder => PageKind::Folder,
        }
    }
}
```

4. Add the two functions right before `async fn run(cli: Cli) -> anyhow::Result<ExitCode> {`:

```rust
/// The refusals of `page set` (reserved, out of range, taken, unknown id) are usage errors
/// (exit 2); any other error stays fatal (exit 1).
fn page_error(e: ressac_core::Error) -> anyhow::Error {
    use ressac_core::Error;
    match e {
        Error::PageReserved(_)
        | Error::PageOutOfRange(_)
        | Error::PageTaken { .. }
        | Error::UnknownPageTarget { .. } => usage(e.to_string()),
        other => other.into(),
    }
}

/// `ressac page list | set`: one line (or one `page` event) per page.
fn page_command(
    app: &Ressac,
    action: PageCommand,
    json: bool,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    match action {
        PageCommand::List => {
            for row in app.pages()? {
                let text = output::page_text(&row);
                output::write_result(out, json, &Event::Page(row), &text)?;
            }
        }
        PageCommand::Set { kind, id, page } => {
            let kind = PageKind::from(kind);
            app.set_page(kind, id, page).map_err(page_error)?;
            let row = app
                .pages()?
                .into_iter()
                .find(|r| r.kind == kind && r.id == Some(id))
                .context("the renumbered page is missing from v_page")?;
            let text = output::page_text(&row);
            output::write_result(out, json, &Event::Page(row), &text)?;
        }
    }
    Ok(())
}
```

5. In `run`, add the arm after `Command::Migrate { … }`, as the last arm of `match cli.command`:

```rust
        Command::Page { action } => {
            let Some(app) = open_with_feeds(&db)? else {
                return Ok(ExitCode::SUCCESS);
            };
            page_command(&app, action, cli.json, &mut out)?;
        }
```

- [ ] **Step 4: Run.** `cargo test -p ressac` → `34 passed` (unit) and `17 passed` (`cli`).

- [ ] **Step 5: Commit.**

```bash
cargo fmt --all
git add crates/ressac/src/output.rs crates/ressac/src/main.rs crates/ressac/tests/cli.rs
git commit -m "ressac page list and ressac page set, with page events under --json" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

### Verification gate

```bash
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All green: `ressac` 34, `cli` 17, lib `250 passed; 0 failed; 1 ignored`, `contract` 8, `end_to_end` 4. If `cargo fmt` reports anything, run `cargo fmt --all` and commit `"cargo fmt"` with the trailer. Then a smoke check in a temp dir. Never use the real `~/Library` paths:

```bash
cargo build -q -p ressac
T=$(mktemp -d)
r() { HOME="$T" target/debug/ressac --db "$T/db.sqlite" "$@"; }   # HOME only for the binary, never for cargo
r migrate                                   # Schema version 3.
r import tests/fixtures/vienna-like.opml
r page list | sed -n '9,11p'                # 200 folder 1 Blogs / 201 feed 1 xkcd.com / 202 folder 2 potes
r --json page set feed 2 412                # {"event":"page","page":412,"kind":"feed","id":2,"title":"Orange Drenka","parent_page":202}
r page set feed 2 800; echo "exit $?"       # Page 800 is reserved. / exit 2
r page --help                               # lists list and set
rm -rf "$T"
```

Then push (Global Constraints). **Stop and wait for Clément's approval before Phase 3.**

### Notes for the executor

- **Verified on 2026-10-14** in a scratch copy of the repository on top of Phase 1, task by task: each task's end state passes the three gate commands, and the counts and the smoke output above come from that run.
- **clap 4.6.7:** `page` alone exits `2` (missing subcommand, help on stderr); `page set entry 1 300` exits `2` (`InvalidValue`); `page set feed 1 70000` exits `2` (`ValueValidation`, `page` is a `u16`); `-3` is read as an unknown flag. `--json` is global, so `page list --json` works too.
- **Ranges are checked by the core, not by clap.** A `u16` such as `900` or `100` parses, then `pages::set` refuses it with its own sentence; only numbers that do not fit a `u16` stop at clap.
- **`page set` prints the row re-read from `v_page`,** so `parent_page` is current. After `page set folder 4 450`, Infos' feeds show `parent_page: 450` (checked in `page_set_renumbers_and_prints_the_row`).
- **The app does not call these commands.** It reads `v_page` through GRDB (Phase 6). The CLI is for the terminal and scripts, which is why both subcommands keep the first-run hint like the other commands instead of creating a database.
- **`Error::Invalid` for `PageKind::Reserved`** cannot come from the CLI (clap only produces `feed` and `folder`), so `page_error` leaves it fatal.
- **Concurrency:** `pages::set` opens its own `IMMEDIATE` transaction, so a `page set` while the app reads, or while a `fetch` writes, waits on the busy timeout (5 s) instead of failing. The app never writes `page`.
- **`main.rs` ends at 667 lines.** A later phase that restructures it (2b, the terminal reader) should first move the command handlers (`garden_command`, `page_command`, `subset`, `check_decision`) into their own module, under the Step 0 rule.
