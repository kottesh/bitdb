/// Integration tests for the TUI App state machine.
///
/// All tests operate on `App` directly - no terminal, no rendering.
/// This covers command dispatch, history navigation, output growth,
/// stats refresh, and clear behaviour.
use bitdb::tui::app::{App, LineKind};
use tempfile::TempDir;

fn make_app(dir: &TempDir) -> App {
    App::new(dir.path()).expect("app should open")
}

// ---- help -------------------------------------------------------------------

#[test]
fn help_command_appends_command_list_to_output() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("help");
    app.submit();

    let output = app.output_lines();
    // The submitted prompt must appear.
    assert!(
        output.iter().any(|l| l.text.contains("help")),
        "prompt line missing"
    );
    // At least put/get/delete/stats/merge/clear/quit must be described.
    let joined: String = output
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for cmd in &["put", "get", "delete", "stats", "merge", "clear", "quit"] {
        assert!(
            joined.contains(cmd),
            "help output missing description for {cmd}"
        );
    }
}

// ---- put / get --------------------------------------------------------------

#[test]
fn put_then_get_returns_value() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("put hello world");
    app.submit();

    app.set_input("get hello");
    app.submit();

    let output = app.output_lines();
    let joined: String = output
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("OK"), "put should print OK");
    assert!(joined.contains("world"), "get should print value");
}

#[test]
fn get_missing_key_prints_not_found() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("get nosuchkey");
    app.submit();

    let joined: String = app
        .output_lines()
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("NOT_FOUND"));
}

// ---- delete -----------------------------------------------------------------

#[test]
fn delete_removes_key() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("put k v");
    app.submit();
    app.set_input("delete k");
    app.submit();
    app.set_input("get k");
    app.submit();

    let joined: String = app
        .output_lines()
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("NOT_FOUND"),
        "deleted key must return NOT_FOUND"
    );
}

// ---- stats ------------------------------------------------------------------

#[test]
fn stats_command_prints_live_keys_and_tombstones() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("put a 1");
    app.submit();
    app.set_input("put b 2");
    app.submit();
    app.set_input("stats");
    app.submit();

    let joined: String = app
        .output_lines()
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("live_keys"),
        "stats output missing live_keys"
    );
    assert!(
        joined.contains("tombstones"),
        "stats output missing tombstones"
    );
}

#[test]
fn stats_bar_refreshes_after_put() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    let before = app.stats_bar();
    app.set_input("put x 1");
    app.submit();
    let after = app.stats_bar();

    // live_keys count must have increased.
    assert_ne!(before, after, "stats bar should refresh after put");
}

// ---- merge ------------------------------------------------------------------

#[test]
fn merge_command_prints_ok() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("put m1 v1");
    app.submit();
    app.set_input("merge");
    app.submit();

    let joined: String = app
        .output_lines()
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("OK"), "merge should print OK");
}

// ---- clear ------------------------------------------------------------------

#[test]
fn clear_command_empties_output() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("put foo bar");
    app.submit();
    assert!(
        !app.output_lines().is_empty(),
        "output should be non-empty before clear"
    );

    app.set_input("clear");
    app.submit();
    assert!(
        app.output_lines().is_empty(),
        "output should be empty after clear"
    );
}

// ---- input editing ----------------------------------------------------------

#[test]
fn backspace_removes_last_character() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("helo");
    app.backspace();
    assert_eq!(app.current_input(), "hel");
}

#[test]
fn submit_clears_input_line() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("get foo");
    app.submit();
    assert!(
        app.current_input().is_empty(),
        "input must be cleared after submit"
    );
}

// ---- command history navigation ---------------------------------------------

#[test]
fn arrow_up_recalls_previous_command() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("put a 1");
    app.submit();
    app.set_input("get a");
    app.submit();

    app.history_prev();
    assert_eq!(app.current_input(), "get a");

    app.history_prev();
    assert_eq!(app.current_input(), "put a 1");
}

#[test]
fn arrow_down_moves_forward_in_history() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("put a 1");
    app.submit();
    app.set_input("get a");
    app.submit();

    app.history_prev();
    app.history_prev();
    app.history_next();
    assert_eq!(app.current_input(), "get a");
}

#[test]
fn arrow_down_past_end_clears_input() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("put a 1");
    app.submit();

    app.history_prev();
    app.history_next();
    assert!(
        app.current_input().is_empty(),
        "past end of history should give empty input"
    );
}

// ---- unknown command --------------------------------------------------------

#[test]
fn unknown_command_prints_error_line() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("frobnicate");
    app.submit();

    let joined: String = app
        .output_lines()
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.to_lowercase().contains("unknown") || joined.to_lowercase().contains("error"),
        "unknown command should print an error"
    );
}

// ---- output line kinds ------------------------------------------------------

#[test]
fn prompt_lines_are_tagged_as_prompt_kind() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.set_input("get foo");
    app.submit();

    let has_prompt = app
        .output_lines()
        .iter()
        .any(|l| l.kind == LineKind::Prompt);
    assert!(has_prompt, "at least one line should be tagged Prompt");
}

#[test]
fn empty_input_submit_does_not_add_output_line() {
    let dir = TempDir::new().unwrap();
    let mut app = make_app(&dir);

    app.submit();
    assert!(
        app.output_lines().is_empty(),
        "empty submit should not add any output"
    );
}
