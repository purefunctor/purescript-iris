use clap::Parser;
use console::style;

use compiler_scripts::test_runner::{
    DeleteFixtureOutcome, RunArgs, TestCategory, accept_category, create_fixture, delete_fixture,
    reject_category, run_category,
};

#[derive(Parser)]
#[command(about = "Compiler development scripts")]
struct Cli {
    /// Test category: compiler (c), format, lowering (l), resolving (r), lsp, docs
    category: TestCategory,

    #[command(flatten)]
    args: RunArgs,
}

fn main() {
    let cli = Cli::parse();

    if let Some(name) = &cli.args.create {
        if let Err(error) = create_fixture(cli.category, name) {
            eprintln!("{:#}", error);
            std::process::exit(1);
        }
        return;
    }

    if let Some(name) = &cli.args.delete {
        match delete_fixture(cli.category, name, cli.args.confirm) {
            Ok(DeleteFixtureOutcome { fixture_paths, snapshot_paths, confirmed }) => {
                if confirmed {
                    for path in &fixture_paths {
                        println!(
                            "{} {}",
                            style("DELETED").red().bold(),
                            style(path.display()).cyan()
                        );
                    }
                    for path in &snapshot_paths {
                        println!(
                            "{} {}",
                            style("DELETED").red().bold(),
                            style(path.display()).cyan()
                        );
                    }
                } else {
                    println!(
                        "{} pending deletion(s) in {}",
                        fixture_paths.len() + snapshot_paths.len(),
                        style(cli.category.as_str()).cyan()
                    );
                    println!();
                    for path in &fixture_paths {
                        println!("  {}", style(path.display()).dim());
                    }
                    for path in &snapshot_paths {
                        println!("  {}", style(path.display()).dim());
                    }
                    println!();
                    println!(
                        "To delete, run: {}",
                        style(format!(
                            "just t {} --delete \"{}\" --confirm",
                            cli.category.as_str(),
                            name
                        ))
                        .cyan()
                    );
                }
            }
            Err(error) => {
                eprintln!("{:#}", error);
                std::process::exit(1);
            }
        }
        return;
    }

    if cli.args.accept && cli.args.reject {
        eprintln!("--accept and --reject cannot be used together");
        std::process::exit(2);
    }

    let result = if cli.args.accept {
        accept_category(cli.category, &cli.args.filters, cli.args.confirm)
    } else if cli.args.reject {
        reject_category(cli.category, &cli.args.filters)
    } else {
        run_category(cli.category, &cli.args)
    };
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("{:#}", error);
            std::process::exit(1);
        }
    }
}
