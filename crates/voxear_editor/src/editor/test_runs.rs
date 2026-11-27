//! Runs the game's `[Test]` methods the Tests window asks for (see
//! `tests_panel`), one a frame so the window shows them going by: in the
//! running game while it plays, and otherwise against the edited level,
//! what a test changed one step to undo.

use std::time::Instant;

use super::Editor;
use crate::tests_panel::{Outcome, TestKey};

impl Editor {
    /// Takes what the Tests window asked for, and runs the next test waiting.
    pub(super) fn update_tests(&mut self) {
        let tests = self.inspector.types().tests.clone();
        let asked = self.tests.update(&mut self.ui, &tests);
        for test in asked {
            if !self.test_queue.contains(&test) {
                self.test_queue.push_back(test);
            }
        }
        let Some((type_name, method)) = self.test_queue.front().cloned() else { return };
        let started = Instant::now();
        // None while the scripts build: the test waits for them.
        let Some(ran) = self.run_script_test(&type_name, &method) else { return };
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        self.test_queue.pop_front();
        let outcome = match ran {
            Ok(Ok(())) => Outcome::Passed { ms },
            Ok(Err(message)) => Outcome::Failed { message, ms },
            Err(error) => Outcome::Failed { message: format!("{error:#}"), ms },
        };
        self.tests.finished(&mut self.ui, (type_name, method), outcome);
    }

    /// Asks for tests to run, as the Tests window's buttons do, by where
    /// they are; `None` runs every test.
    pub fn run_tests(&mut self, tests: Option<Vec<TestKey>>) {
        let all = || self.inspector.types().tests.iter().map(|t| (t.type_name.clone(), t.method.clone())).collect();
        for test in tests.unwrap_or_else(all) {
            if !self.test_queue.contains(&test) {
                self.tests.finished(&mut self.ui, test.clone(), Outcome::Waiting);
                self.test_queue.push_back(test);
            }
        }
    }

    /// How each test last came out, in the order they run, with whether
    /// any still wait to run.
    pub fn test_outcomes(&self) -> (Vec<(voxear_engine::script_types::ScriptTest, Outcome)>, bool) {
        let outcomes = self.tests.outcomes().into_iter().map(|(test, outcome)| (test.clone(), outcome)).collect();
        (outcomes, !self.test_queue.is_empty())
    }
}
