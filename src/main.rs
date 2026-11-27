use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use researchops::{
    engine::{Engine, ResearchSpec},
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
    let mut engine = Engine::open(&cli.db)?;
    match cli.command {
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
            let report = engine.report(&run)?;
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
        Commands::Serve { .. } => unreachable!(),
    }
    Ok(())
}
