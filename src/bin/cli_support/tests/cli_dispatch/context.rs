use super::super::super::cli_args::{BenchCommand, Command, StatsCommand};
use super::parse;

#[test]
fn context_aliases_parse_numeric_budget() {
    let cards = parse(&["cards", "--query", "where is auth", "--budget", "1500"]);
    let Some(Command::Cards { query, budget }) = cards.command else {
        panic!("cards should parse");
    };
    assert_eq!(query, "where is auth");
    assert_eq!(budget, Some(1500));

    let explain = parse(&["explain", "src/lib.rs", "--budget", "1000"]);
    assert!(matches!(
        explain.command,
        Some(Command::Explain {
            budget: Some(1000),
            ..
        })
    ));

    let impact = parse(&["impact", "src/lib.rs", "--budget", "2000"]);
    assert!(matches!(
        impact.command,
        Some(Command::Impact {
            budget: Some(2000),
            ..
        })
    ));

    let tests = parse(&["tests", "src/lib.rs", "--budget", "1200"]);
    assert!(matches!(
        tests.command,
        Some(Command::Tests {
            budget: Some(1200),
            ..
        })
    ));

    let risks = parse(&["risks", "src/lib.rs", "--budget", "1200"]);
    assert!(matches!(
        risks.command,
        Some(Command::Risks {
            budget: Some(1200),
            ..
        })
    ));
}

#[test]
fn stats_and_bench_context_parse() {
    let stats = parse(&["stats", "context", "--json"]);
    assert!(matches!(
        stats.command,
        Some(Command::Stats(StatsCommand::Context { json: true, .. }))
    ));

    let bench = parse(&[
        "bench",
        "context",
        "--tasks",
        "benches/tasks/*.json",
        "--mode",
        "all",
        "--json",
    ]);
    match bench.command {
        Some(Command::Bench(BenchCommand::Context {
            mode, json: true, ..
        })) => assert_eq!(mode, "all"),
        _ => panic!("bench context --mode all --json should parse"),
    }
}
