use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use mc_tunnel::routing::{load_client_config, ProxyMode};
use mc_tunnel::speed::SpeedMode;
use mc_tunnel::{init_logging, run_client, run_server, stop_proxy};

#[derive(Clone, ValueEnum, Debug)]
enum SpeedArg {
    Stealth,
    Balanced,
    Fast,
}

impl From<SpeedArg> for SpeedMode {
    fn from(v: SpeedArg) -> Self {
        match v {
            SpeedArg::Stealth => SpeedMode::Stealth,
            SpeedArg::Balanced => SpeedMode::Balanced,
            SpeedArg::Fast => SpeedMode::Fast,
        }
    }
}

#[derive(Clone, ValueEnum, Debug)]
enum ProxyModeArg {
    All,
    Gfw,
    Direct,
}

impl From<ProxyModeArg> for ProxyMode {
    fn from(v: ProxyModeArg) -> Self {
        match v {
            ProxyModeArg::All => ProxyMode::All,
            ProxyModeArg::Gfw => ProxyMode::GfwOnly,
            ProxyModeArg::Direct => ProxyMode::Direct,
        }
    }
}

#[derive(Parser)]
#[command(name = "mc-tunnel", about = "Minecraft 流量伪装代理隧道")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Client {
        #[arg(short, long)]
        remote: String,
        #[arg(short, long, default_value = "127.0.0.1:1080")]
        local: String,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        set_proxy: bool,
        #[arg(long, value_enum, default_value_t = ProxyModeArg::Gfw)]
        proxy_mode: ProxyModeArg,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(short = 'S', long, value_enum, default_value_t = SpeedArg::Fast)]
        speed: SpeedArg,
    },
    Server {
        #[arg(short, long, default_value = "0.0.0.0:25565")]
        listen: String,
        #[arg(short = 'S', long, value_enum, default_value_t = SpeedArg::Fast)]
        speed: SpeedArg,
    },
    Stop,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let cli = Cli::parse();

    match cli.command {
        Commands::Client {
            remote,
            local,
            set_proxy,
            proxy_mode,
            config,
            speed,
        } => {
            let cli_mode: ProxyMode = proxy_mode.into();
            let mut policy = if let Some(path) = config.as_ref() {
                load_client_config(path)?.policy()
            } else {
                mc_tunnel::routing::ProxyPolicy {
                    mode: cli_mode,
                    whitelist: Vec::new(),
                }
            };
            if config.is_none() {
                policy.mode = cli_mode;
            }
            run_client(&remote, &local, policy, set_proxy, speed.into()).await
        }
        Commands::Server { listen, speed } => run_server(&listen, speed.into()).await,
        Commands::Stop => stop_proxy(),
    }
}
