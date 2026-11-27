//! The Tests window: the game's `[Test]` methods (see `Voxear.Testing` in
//! Voxear.Core), by category, to run all of them, a category's, or one.
//! Categories come in the order of their first test, then by name; tests
//! in a category by their `Order`, then by title. Each test shows whether
//! it passed, failed, waits or runs, how long it took, and what a failure
//! said under it. A filter narrows the list by title or category.
//!
//! The window asks the editor to run tests through [`TestsPanel::update`]:
//! the editor runs one a frame (see `editor::tests`), so the list shows
//! them going by, and hands each outcome back with
//! [`TestsPanel::finished`].

use std::collections::HashMap;

use voxear_engine::script_types::ScriptTest;
use voxear_ui::{NodeId, Style, Ui};

/// How a test came out, or where it is.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    NotRun,
    Waiting,
    Passed { ms: f32 },
    Failed { message: String, ms: f32 },
}

impl Outcome {
    fn class(&self) -> &'static str {
        match self {
            Outcome::NotRun => "test-not-run",
            Outcome::Waiting => "test-waiting",
            Outcome::Passed { .. } => "test-passed",
            Outcome::Failed { .. } => "test-failed",
        }
    }
}

/// A test by where it is: its type's full name and its method.
pub type TestKey = (String, String);

fn key(test: &ScriptTest) -> TestKey {
    (test.type_name.clone(), test.method.clone())
}

/// One test's row: its mark, title, time, run button and failure line.
struct TestRow {
    key: TestKey,
    mark: NodeId,
    time: NodeId,
    run: NodeId,
    failure: NodeId,
}

/// One category's heading: its name, how its tests came out, and its run button.
struct CategoryRow {
    tests: Vec<TestKey>,
    counts: NodeId,
    run: NodeId,
}

pub struct TestsPanel {
    run_all: NodeId,
    filter: NodeId,
    summary: NodeId,
    list: NodeId,
    empty: NodeId,
    /// The tests as last listed, in the order they run.
    tests: Vec<ScriptTest>,
    rows: Vec<TestRow>,
    categories: Vec<CategoryRow>,
    outcomes: HashMap<TestKey, Outcome>,
    shown_filter: String,
}

impl TestsPanel {
    pub fn new(ui: &mut Ui, parent: NodeId) -> Self {
        let bar = ui.panel(parent, Style::row());
        ui.add_class(bar, "tests-bar");
        let run_all = ui.button(bar, "Run All");
        ui.add_class(run_all, "primary");
        ui.add_class(run_all, "tests-run-all");
        let filter = ui.text_field(bar, "");
        ui.add_class(filter, "tests-filter");
        ui.set_placeholder(filter, "Filter");
        let summary = ui.add_label(parent, "");
        ui.add_class(summary, "tests-summary");
        let list = ui.panel(parent, Style::column().scroll());
        ui.add_class(list, "fill");
        ui.add_class(list, "tests-list");
        let empty = ui.add_label(
            parent,
            "No tests yet. Mark a static method that takes nothing with [Test] from Voxear.Testing, and check what it does with Assert.",
        );
        ui.add_class(empty, "tests-empty");
        Self {
            run_all,
            filter,
            summary,
            list,
            empty,
            tests: Vec::new(),
            rows: Vec::new(),
            categories: Vec::new(),
            outcomes: HashMap::new(),
            shown_filter: String::new(),
        }
    }

    /// Lists the tests the scripts have, again when they or the filter
    /// changed, and gives back those asked to run, in order.
    pub fn update(&mut self, ui: &mut Ui, tests: &[ScriptTest]) -> Vec<TestKey> {
        let filter = ui.text(self.filter).trim().to_lowercase();
        let ordered = ordered(tests);
        if ordered != self.tests || filter != self.shown_filter {
            self.tests = ordered;
            self.shown_filter = filter;
            self.build(ui);
        }
        let shown: Vec<TestKey> = self.rows.iter().map(|row| row.key.clone()).collect();
        let mut asked = Vec::new();
        if ui.clicked(self.run_all) {
            asked = shown;
        } else if let Some(category) = self.categories.iter().find(|c| ui.clicked(c.run)) {
            asked = category.tests.clone();
        } else if let Some(row) = self.rows.iter().find(|row| ui.clicked(row.run)) {
            asked = vec![row.key.clone()];
        }
        for test in &asked {
            self.outcomes.insert(test.clone(), Outcome::Waiting);
        }
        if !asked.is_empty() {
            self.show(ui);
        }
        asked
    }

    /// Notes how a test came out.
    pub fn finished(&mut self, ui: &mut Ui, test: TestKey, outcome: Outcome) {
        self.outcomes.insert(test, outcome);
        self.show(ui);
    }

    /// How each test last came out, in the order they run.
    pub fn outcomes(&self) -> Vec<(&ScriptTest, Outcome)> {
        self.tests
            .iter()
            .map(|test| (test, self.outcomes.get(&key(test)).cloned().unwrap_or(Outcome::NotRun)))
            .collect()
    }

    fn build(&mut self, ui: &mut Ui) {
        for child in ui.children(self.list) {
            ui.remove(child);
        }
        self.rows.clear();
        self.categories.clear();
        let filter = self.shown_filter.clone();
        let shown: Vec<ScriptTest> = self
            .tests
            .iter()
            .filter(|t| {
                filter.is_empty()
                    || t.title.to_lowercase().contains(&filter)
                    || t.category.to_lowercase().contains(&filter)
            })
            .cloned()
            .collect();
        let mut at = 0;
        while at < shown.len() {
            let category = shown[at].category.clone();
            let inside: Vec<&ScriptTest> = shown[at..].iter().take_while(|t| t.category == category).collect();
            at += inside.len();
            let heading = ui.panel(self.list, Style::row());
            ui.add_class(heading, "tests-category");
            let name = ui.add_label(heading, &category);
            ui.add_class(name, "tests-category-name");
            let counts = ui.add_label(heading, "");
            ui.add_class(counts, "tests-category-counts");
            let run = ui.icon_button(heading, "play");
            ui.add_class(run, "tests-run");
            ui.set_tooltip(run, &format!("Run {category}"), None);
            self.categories.push(CategoryRow { tests: inside.iter().map(|t| key(t)).collect(), counts, run });
            for test in inside {
                let block = ui.panel(self.list, Style::column());
                ui.add_class(block, "tests-test");
                let line = ui.panel(block, Style::row());
                ui.add_class(line, "tests-test-line");
                let mark = ui.panel(line, Style::default());
                ui.add_class(mark, "tests-mark");
                let title = ui.add_label(line, &test.title);
                ui.add_class(title, "tests-title");
                ui.set_tooltip(title, &format!("{}.{}", test.type_name, test.method), None);
                let time = ui.add_label(line, "");
                ui.add_class(time, "tests-time");
                let run = ui.icon_button(line, "play");
                ui.add_class(run, "tests-run");
                ui.set_tooltip(run, "Run", None);
                let failure = ui.add_label(block, "");
                ui.add_class(failure, "tests-failure");
                self.rows.push(TestRow { key: key(test), mark, time, run, failure });
            }
        }
        ui.set_visible(self.empty, self.tests.is_empty());
        self.show(ui);
    }

    /// Shows each test's outcome, each category's counts and the summary.
    fn show(&mut self, ui: &mut Ui) {
        let outcome = |test: &TestKey| self.outcomes.get(test).cloned().unwrap_or(Outcome::NotRun);
        for row in &self.rows {
            let now = outcome(&row.key);
            for class in ["test-not-run", "test-waiting", "test-passed", "test-failed"] {
                ui.set_class(row.mark, class, now.class() == class);
            }
            let (time, failure) = match &now {
                Outcome::Passed { ms } => (format!("{ms:.0} ms"), String::new()),
                Outcome::Failed { message, ms } => (format!("{ms:.0} ms"), message.clone()),
                Outcome::Waiting => ("…".into(), String::new()),
                Outcome::NotRun => (String::new(), String::new()),
            };
            ui.set_text(row.time, &time);
            ui.set_visible(row.failure, !failure.is_empty());
            ui.set_text(row.failure, &failure);
        }
        for category in &self.categories {
            ui.set_text(category.counts, &counts(category.tests.iter().map(|t| outcome(t))));
        }
        ui.set_text(self.summary, &counts(self.tests.iter().map(|t| outcome(&key(t)))));
    }
}

/// The tests in the order they run: categories by their first test's
/// order, then by name; in a category by order, then title.
fn ordered(tests: &[ScriptTest]) -> Vec<ScriptTest> {
    let mut first: HashMap<&str, i32> = HashMap::new();
    for test in tests {
        let at = first.entry(&test.category).or_insert(test.order);
        *at = (*at).min(test.order);
    }
    let mut sorted = tests.to_vec();
    sorted.sort_by(|a, b| {
        (first[a.category.as_str()], &a.category, a.order, &a.title).cmp(&(
            first[b.category.as_str()],
            &b.category,
            b.order,
            &b.title,
        ))
    });
    sorted
}

/// "3 passed · 1 failed · 2 not run", leaving out what is none.
fn counts(outcomes: impl Iterator<Item = Outcome>) -> String {
    let (mut passed, mut failed, mut waiting, mut not_run) = (0, 0, 0, 0);
    for outcome in outcomes {
        match outcome {
            Outcome::Passed { .. } => passed += 1,
            Outcome::Failed { .. } => failed += 1,
            Outcome::Waiting => waiting += 1,
            Outcome::NotRun => not_run += 1,
        }
    }
    let parts = [(passed, "passed"), (failed, "failed"), (waiting, "running"), (not_run, "not run")];
    let said: Vec<String> = parts.iter().filter(|(n, _)| *n > 0).map(|(n, what)| format!("{n} {what}")).collect();
    said.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test(title: &str, category: &str, order: i32) -> ScriptTest {
        ScriptTest {
            title: title.into(),
            category: category.into(),
            order,
            type_name: "Game.Tests".into(),
            method: title.replace(' ', ""),
        }
    }

    #[test]
    fn categories_run_in_the_order_of_their_first_test_and_tests_by_order_then_title() {
        let tests = [test("B", "Saving", 0), test("Z", "Doors", -1), test("A", "Saving", 0), test("Y", "Doors", 5)];
        let titles: Vec<String> = ordered(&tests).into_iter().map(|t| format!("{}/{}", t.category, t.title)).collect();
        assert_eq!(titles, ["Doors/Z", "Doors/Y", "Saving/A", "Saving/B"]);
    }

    #[test]
    fn counts_say_only_what_there_is() {
        let outcomes = [Outcome::Passed { ms: 1.0 }, Outcome::Passed { ms: 2.0 }, Outcome::NotRun];
        assert_eq!(counts(outcomes.into_iter()), "2 passed · 1 not run");
    }
}
