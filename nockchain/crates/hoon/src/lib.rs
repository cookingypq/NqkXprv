// Execute nock scripts
use std::fs::File;

use clap::{arg, command, Parser};
use hoonc::save_generator;
use nockapp::utils::NOCK_STACK_SIZE;
use nockvm::interpreter::Context;
use nockvm::jets::cold::Cold;
use nockvm::jets::hot::{HotEntry, URBIT_HOT_STATE};
use nockvm::mem::NockStack;
use nockvm::trace::{JsonBackend, TraceInfo};

/// Command line arguments
#[derive(Parser, Debug, Clone)]
#[command(name = "hoon")]
pub struct HoonCli {
    #[command(flatten)]
    pub boot: nockapp::kernel::boot::Cli,
    #[arg(help = "Nock script to execute")]
    pub nock_script: std::path::PathBuf,
    #[arg(help = "Dependency directory")]
    pub dep_dir: std::path::PathBuf,
    #[arg(
        long,
        help = "Where to save the output of the kicked jam",
        default_value = None
    )]
    pub out_dir: Option<std::path::PathBuf>,
}

pub async fn run(cli: HoonCli, hot_state: &[HotEntry]) -> Result<(), Box<dyn std::error::Error>> {
    // let trace_info = if cli.boot.trace_opts {
    //     let file = File::create("trace.json").expect("Cannot create trace file trace.json");
    //     let pid = std::process::id();
    //     let process_start = std::time::Instant::now();
    //     Some(TraceInfo {
    //         file,
    //         pid,
    //         process_start,
    //     })
    // } else {
    //     None
    // };
    let trace_info = if let Some(trace_mode) = cli.boot.trace_opts.mode {
        match trace_mode {
            nockapp::kernel::boot::TraceMode::Json => {
                let file = File::create("trace.json").expect("Cannot create trace file trace.json");
                let pid = std::process::id();
                let process_start = std::time::Instant::now();
                Some(
                    JsonBackend {
                        file,
                        pid,
                        process_start,
                    }
                    .into(),
                )
            }
            _ => None,
        }
        // let file = File::create("trace.json").expect("Cannot create trace file trace.json");
        // let pid = std::process::id();
        // let process_start = std::time::Instant::now();
        // let json_backend = JsonBackend {
        //     file,
        //     pid,
        //     process_start,
        // };
        // Some(json_backend.into())
    } else {
        None
    };
    let mut context: Context = init_context(Some(hot_state), trace_info);

    save_generator(&mut context, &cli.nock_script, cli.dep_dir, cli.out_dir).await
}

/// Initializes a nockvm interpreter Context with default settings
fn init_context(extra_hot_state: Option<&[HotEntry]>, trace_info: Option<TraceInfo>) -> Context {
    let mut stack: NockStack = NockStack::new(NOCK_STACK_SIZE, 0);
    let constant_hot_state = if let Some(hot_state) = extra_hot_state {
        [URBIT_HOT_STATE, hot_state].concat()
    } else {
        [URBIT_HOT_STATE].concat()
    };
    let cold = Cold::new(&mut stack);
    nockapp::utils::create_context(stack, &constant_hot_state, cold, trace_info, vec![])
}
