use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use researchops::{
    contracts::{Binding, ContractImport, Mode, Trigger},
    engine::{Engine, ResearchSpec},
    execution::{self, SystemClock},
    provider::{ConnectionConfig, Provider},
    web,
};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    version,
    about = "ResearchOps — durable, evidence-tracked local research"
)]
struct Cli {
    #[arg(long, global = true, default_value = "data/researchops.db")]
    db: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute continuously across batches under the bound contract.
    Execute {
        #[arg(long)]
        run: String,
        #[arg(long, value_enum, default_value = "local")]
        mode: Mode,
        #[arg(long, value_enum, default_value = "manual")]
        trigger: Trigger,
    },
    /// Inspect unfinished runs and safe recovery files, even if the DB is unavailable.
    Recovery {
        #[arg(long)]
        run: Option<String>,
    },
    /// Register or list version-pinned execution policies.
    Contracts {
        #[command(subcommand)]
        action: ContractAction,
    },
    /// Bind a contract before a run starts.
    Contract {
        #[command(subcommand)]
        action: BindingAction,
    },
    /// Open the local dashboard. No API keys or external services required.
    Serve {
        #[arg(long, default_value_t = 8787)]
        port: u16,
    },
    /// Create a run from a JSON research specification.
    Create {
        #[arg(long)]
        spec: PathBuf,
    },
    /// Process pending tasks; resume expired worker leases automatically.
    Work {
        #[arg(long)]
        run: String,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = 60)]
        lease_seconds: i64,
    },
    /// Inspect a run, including task attempts, gaps and evidence.
    Status {
        #[arg(long)]
        run: String,
    },
    /// List the latest 100 runs.
    List,
    /// Export an evidence report to stdout or a Markdown file.
    Report {
        #[arg(long)]
        run: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Run the bundled synthetic example end to end.
    Demo,
}

#[derive(Subcommand)]
enum ContractAction {
    Import {
        #[arg(long)]
        file: PathBuf,
    },
    List,
}
#[derive(Subcommand)]
enum BindingAction {
    Bind {
        #[arg(long)]
        run: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        md5: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Commands::Serve { port } = cli.command {
        return web::serve(
            cli.db,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        )
        .await;
    }
    if let Commands::Recovery { run } = &cli.command {
        let database = Engine::open(&cli.db).and_then(|e| {
            if let Some(id) = run {
                Ok(serde_json::json!([{"run_id":id,"execution":e.execution_view(id)?}]))
            } else {
                Ok(serde_json::json!(e.recovery_candidates()?))
            }
        });
        let output = match database {
            Ok(runs) => {
                serde_json::json!({"runs":runs,"files":execution::read_recovery_files(&cli.db,run.as_deref())})
            }
            Err(e) => {
                serde_json::json!({"runs":[],"files":execution::read_recovery_files(&cli.db,run.as_deref()),"database_error":execution::redact(&e.to_string(),None)})
            }
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }
    if let Commands::Execute { run, mode, trigger } = &cli.command {
        let mut provider = None;
        if *mode == Mode::Agents {
            // Validate the contract before any connection test can call a model.
            let mut engine = Engine::open(&cli.db)?;
            let policy = engine.authorize_mode(run, *mode)?;
            if policy.profile != researchops::contracts::Profile::WeeklyReport {
                if let (Ok(base_url), Ok(researcher_model), Ok(verifier_model)) = (
                    std::env::var("RESEARCHOPS_API_BASE_URL"),
                    std::env::var("RESEARCHOPS_RESEARCHER_MODEL"),
                    std::env::var("RESEARCHOPS_VERIFIER_MODEL"),
                ) {
                    let mut configured = Provider::new(ConnectionConfig {
                        base_url,
                        researcher_model,
                        verifier_model,
                        api_key: std::env::var("RESEARCHOPS_API_KEY").unwrap_or_default(),
                    })?;
                    configured.test().await?;
                    configured.tested = true;
                    provider = Some(configured);
                }
            }
        }
        println!(
            "{}",
            serde_json::to_string_pretty(
                &execution::execute(
                    cli.db,
                    run.clone(),
                    *mode,
                    *trigger,
                    provider,
                    SystemClock::default()
                )
                .await?
            )?
        );
        return Ok(());
    }
    let mut engine = Engine::open(&cli.db)?;
    match cli.command {
        Commands::Contracts { action } => match action {
            ContractAction::Import { file } => {
                let input = std::fs::read_to_string(file)?;
                let spec: ContractImport = serde_json::from_str(&input)?;
                engine.register_contract(spec)?;
                println!("Contract registered and read back.");
            }
            ContractAction::List => {
                println!("{}", serde_json::to_string_pretty(&engine.contracts()?)?)
            }
        },
        Commands::Contract {
            action: BindingAction::Bind { run, version, md5 },
        } => {
            engine.bind_contract(
                &run,
                Binding {
                    version,
                    expected_md5: md5,
                },
            )?;
            println!("Contract bound.");
        }
        Commands::Create { spec } => {
            let content = std::fs::read_to_string(spec)?;
            if content.len() > 6_000_000 {
                bail!("specification must not exceed 6 MB");
            }
            let id = engine.create(serde_json::from_str::<ResearchSpec>(&content)?)?;
            println!("{id}");
        }
        Commands::Work {
            run,
            limit,
            lease_seconds,
        } => println!(
            "{}",
            serde_json::to_string_pretty(&engine.work(&run, limit, lease_seconds)?)?
        ),
        Commands::Status { run } => {
            println!("{}", serde_json::to_string_pretty(&engine.get(&run)?)?)
        }
        Commands::List => println!("{}", serde_json::to_string_pretty(&engine.list()?)?),
        Commands::Report { run, output } => {
            let report = engine.materialize_report(&run)?;
            if let Some(path) = output {
                std::fs::write(&path, report)?;
                println!("Saved {}", path.display());
            } else {
                print!("{report}");
            }
        }
        Commands::Demo => {
            let id = engine.create(serde_json::from_str(include_str!(
                "../examples/research.json"
            ))?)?;
            engine.work(&id, 100, 60)?;
            print!("{}", engine.report(&id)?);
        }
        Commands::Serve { .. } | Commands::Execute { .. } | Commands::Recovery { .. } => {
            unreachable!()
        }
    }
    Ok(())
}
