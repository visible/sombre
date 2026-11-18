use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub struct Outcome {
    pub stopped: bool,
    pub refused: bool,
    pub failed: bool,
    pub steps: usize,
    pub said: String,
    pub goal: Option<(String, String)>,
    pub limited: bool,
}

pub const TURNS: usize = 20;
const IDLE: usize = 3;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Show,
    Clear,
    Pause,
    Resume,
    Set(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Active,
    Paused,
    Complete,
    Blocked(String),
    Stalled,
    Limit,
}

#[derive(Clone, Debug)]
pub struct Goal {
    pub objective: String,
    pub status: Status,
    pub turns: usize,
    pub idle: usize,
    pub started: Instant,
}

pub fn parse(text: &str) -> Command {
    match text.trim() {
        "" => Command::Show,
        "clear" | "stop" | "off" | "cancel" => Command::Clear,
        "pause" => Command::Pause,
        "resume" | "continue" => Command::Resume,
        objective => Command::Set(objective.to_string()),
    }
}

impl Goal {
    pub fn new(objective: String) -> Self {
        Self {
            idle: 0,
            objective,
            started: Instant::now(),
            status: Status::Active,
            turns: 0,
        }
    }

    pub fn judge(&mut self, outcome: &Outcome) {
        let reported = outcome.goal.clone().or_else(|| marker(&outcome.said));
        self.status = match reported {
            _ if outcome.stopped || outcome.refused => Status::Paused,
            _ if outcome.failed => Status::Blocked("the last turn failed".to_string()),
            Some((status, _)) if status == "complete" => Status::Complete,
            Some((_, reason)) => Status::Blocked(reason),
            None => {
                self.idle = if outcome.steps == 0 { self.idle + 1 } else { 0 };
                if self.idle >= IDLE {
                    Status::Stalled
                } else if self.turns >= TURNS {
                    Status::Limit
                } else {
                    Status::Active
                }
            }
        };
    }

    pub fn native(&mut self, outcome: &Outcome, failing: impl FnOnce() -> Option<String>) {
        self.status = if outcome.limited {
            Status::Limit
        } else if outcome.stopped || outcome.refused {
            Status::Paused
        } else if outcome.failed {
            Status::Blocked("the last turn failed".to_string())
        } else {
            match &outcome.goal {
                Some((status, _)) if status == "paused" => Status::Paused,
                Some((status, reason)) if status != "complete" => Status::Blocked(reason.clone()),
                _ => failing().map_or(Status::Complete, |command| {
                    Status::Blocked(format!("`{command}` still fails"))
                }),
            }
        };
    }

    pub fn summary(&self) -> String {
        let turns = if self.turns == 1 {
            "1 turn".to_string()
        } else {
            format!("{} turns", self.turns)
        };
        let time = elapsed(self.started.elapsed());
        match &self.status {
            Status::Active => format!("pursuing the goal, turn {} of {TURNS}", self.turns + 1),
            Status::Paused => format!("goal paused after {turns}. /goal resume picks it up"),
            Status::Complete => format!("goal achieved in {turns}, {time}"),
            Status::Blocked(reason) if reason.is_empty() => format!("goal blocked after {turns}"),
            Status::Blocked(reason) => format!("goal blocked after {turns}: {reason}"),
            Status::Stalled => format!(
                "goal stalled: {IDLE} turns in a row without progress. /goal resume tries again"
            ),
            Status::Limit => format!("goal stopped after {TURNS} turns. /goal resume keeps going"),
        }
    }

    pub fn word(&self) -> &'static str {
        match self.status {
            Status::Active => "active",
            Status::Paused => "paused",
            Status::Complete => "achieved",
            Status::Blocked(_) => "blocked",
            Status::Stalled => "stalled",
            Status::Limit => "turn limit",
        }
    }
}

pub fn elapsed(time: Duration) -> String {
    let seconds = time.as_secs();
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {}s", seconds / 60, seconds % 60),
        _ => format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60),
    }
}

pub fn checks(objective: &str) -> Vec<String> {
    let parts: Vec<&str> = objective.split('`').collect();
    parts
        .iter()
        .enumerate()
        .filter(|(index, _)| index % 2 == 1 && *index + 1 < parts.len())
        .map(|(_, part)| part.trim())
        .filter(|command| !command.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn retry(command: &str, output: &str, objective: &str) -> String {
    let output: String = output
        .chars()
        .rev()
        .take(3_000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!(
        "I ran `{command}` to check the goal and it failed:\n```\n{}\n```\n\n{}",
        output.trim(),
        onward(objective)
    )
}

pub fn marker(said: &str) -> Option<(String, String)> {
    let squeezed: String = said
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    for status in ["complete", "blocked"] {
        if squeezed.contains(&format!("\"status\":\"{status}\"")) {
            return Some((status.to_string(), String::new()));
        }
    }
    let last = said
        .lines()
        .rev()
        .map(|line| line.trim().trim_matches('`').trim())
        .find(|line| !line.is_empty())?
        .to_lowercase();
    let rest = last.strip_prefix("goal:")?.trim();
    if rest.starts_with("complete") {
        Some(("complete".to_string(), String::new()))
    } else {
        rest.strip_prefix("blocked").map(|reason| {
            (
                "blocked".to_string(),
                reason
                    .trim_start_matches([' ', ':', '-', '—'])
                    .trim()
                    .to_string(),
            )
        })
    }
}

const FINISH: &str = "When it's fully done, call the goal tool with status complete. If you're truly blocked after trying more than once, call the goal tool with status blocked and say why.";

pub fn opening(objective: &str) -> String {
    format!(
        "Goal: {}\n\nStart on it now and keep going without waiting for me. You'll be nudged to continue after each turn until it's finished. {}",
        serde_json::Value::String(objective.to_string()),
        FINISH
    )
}

pub fn onward(objective: &str) -> String {
    format!(
        "Keep going on the goal: {}\n\nLook at the real state first (run it, test it, read it) before you change anything. If the last turn didn't move things forward, change your approach rather than repeating it. Call it done only once you've checked that every part of the goal holds right now. {}",
        serde_json::Value::String(objective.to_string()),
        FINISH
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(steps: usize, said: &str) -> Outcome {
        Outcome {
            said: said.to_string(),
            steps,
            ..Outcome::default()
        }
    }

    #[test]
    fn a_native_goal_that_runs_past_its_turns_stops_and_can_resume() {
        let mut goal = Goal::new("proof.txt holds banana".to_string());
        goal.turns = TURNS;
        goal.native(
            &Outcome {
                limited: true,
                stopped: true,
                ..Outcome::default()
            },
            || Some("never runs".to_string()),
        );
        assert_eq!(goal.status, Status::Limit);
        assert_eq!(
            goal.summary(),
            format!("goal stopped after {TURNS} turns. /goal resume keeps going")
        );
    }

    #[test]
    fn native_goals_trust_the_agent_then_the_checks() {
        let mut goal = Goal::new("x".to_string());
        let reported = |status: &str, reason: &str| Outcome {
            goal: Some((status.to_string(), reason.to_string())),
            ..Outcome::default()
        };
        goal.native(&reported("blocked", "no key"), || None);
        assert_eq!(goal.status, Status::Blocked("no key".to_string()));
        goal.native(&reported("complete", ""), || Some("make test".to_string()));
        assert_eq!(
            goal.status,
            Status::Blocked("`make test` still fails".to_string())
        );
        goal.native(&Outcome::default(), || None);
        assert_eq!(goal.status, Status::Complete);
        goal.native(
            &Outcome {
                stopped: true,
                ..Outcome::default()
            },
            || None,
        );
        assert_eq!(goal.status, Status::Paused);
    }

    #[test]
    fn reads_goal_commands() {
        assert_eq!(parse(""), Command::Show);
        assert_eq!(parse("clear"), Command::Clear);
        assert_eq!(parse("pause"), Command::Pause);
        assert_eq!(parse("resume"), Command::Resume);
        assert_eq!(
            parse("all tests pass"),
            Command::Set("all tests pass".to_string())
        );
    }

    #[test]
    fn reads_codex_completion_markers() {
        assert_eq!(
            marker("done, tests pass\n\n`goal: complete`"),
            Some(("complete".to_string(), String::new()))
        );
        assert_eq!(
            marker("I need an api key.\ngoal: blocked — no key for stripe"),
            Some(("blocked".to_string(), "no key for stripe".to_string()))
        );
        assert_eq!(marker("still working"), None);
        assert_eq!(
            marker("{\"status\": \"complete\", \"evidence\": \"ran it\"}"),
            Some(("complete".to_string(), String::new()))
        );
    }

    #[test]
    fn keeps_going_while_there_is_progress() {
        let mut goal = Goal::new("x".to_string());
        goal.turns = 1;
        goal.judge(&outcome(2, "fixed one test"));
        assert_eq!(goal.status, Status::Active);
    }

    #[test]
    fn stops_when_the_model_reports_or_stalls() {
        let mut goal = Goal::new("x".to_string());
        goal.judge(&Outcome {
            goal: Some(("complete".to_string(), String::new())),
            ..Outcome::default()
        });
        assert_eq!(goal.status, Status::Complete);
        let mut codex = Goal::new("x".to_string());
        codex.judge(&outcome(1, "all green\ngoal: complete"));
        assert_eq!(codex.status, Status::Complete);
        let mut stuck = Goal::new("x".to_string());
        for _ in 0..3 {
            stuck.judge(&outcome(0, "thinking"));
        }
        assert_eq!(stuck.status, Status::Stalled);
    }

    #[test]
    fn pauses_on_stop_and_respects_the_turn_limit() {
        let mut goal = Goal::new("x".to_string());
        goal.judge(&Outcome {
            stopped: true,
            ..Outcome::default()
        });
        assert_eq!(goal.status, Status::Paused);
        goal.turns = TURNS;
        goal.judge(&outcome(1, "more"));
        assert_eq!(goal.status, Status::Limit);
    }

    #[test]
    fn passes_the_objective_as_data() {
        let prompt = opening("say \"hi\"\nthen stop");
        assert!(prompt.starts_with("Goal: \"say \\\"hi\\\"\\nthen stop\""));
        assert!(prompt.contains("goal tool"));
        assert!(onward("x").contains("goal tool"));
    }

    #[test]
    fn finds_check_commands_in_backticks() {
        assert_eq!(
            checks("`bun test` passes and `bun run lint` is clean"),
            ["bun test", "bun run lint"]
        );
        assert!(checks("make it faster").is_empty());
        assert!(checks("odd ` tick").is_empty());
    }

    #[test]
    fn formats_time_used() {
        assert_eq!(elapsed(Duration::from_secs(42)), "42s");
        assert_eq!(elapsed(Duration::from_secs(130)), "2m 10s");
        assert_eq!(elapsed(Duration::from_secs(7_300)), "2h 1m");
    }
}
