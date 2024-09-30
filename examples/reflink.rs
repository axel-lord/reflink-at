//! Simple cli reflink.

use ::std::{
    fs::File,
    os::fd::{AsFd, AsRawFd},
    path::PathBuf,
    process::ExitCode,
};

use ::clap::Parser;
use ::mm_reflink::Mode;
use ::nix::errno::Errno;

/// Create a reflink.
#[derive(Parser)]
#[command(author, version, long_about = None)]
struct Cli {
    /// File to be linked to.
    dest: PathBuf,

    /// Link path.
    src: PathBuf,
}

fn main() -> ExitCode {
    color_eyre::install().unwrap();
    env_logger::init();
    match run(Cli::parse()) {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            log::error!("{err}");
            if let Some(errno) = err.downcast_ref::<Errno>() {
                ExitCode::from(*errno as i32 as u8)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

/// Create the reflink.
///
/// # Errors
/// If a reflink cannot be created.
fn run(Cli { dest, src }: Cli) -> color_eyre::Result<()> {
    let src_file = File::open(src)?;
    let stat = nix::sys::stat::fstat(src_file.as_raw_fd())?;
    let mode = Mode::from_bits_truncate(stat.st_mode);

    mm_reflink::reflink_at(
        None,
        &dest,
        src_file.as_fd(),
        mode,
        ::mm_reflink::OnExists::CreateNewOnly,
    )?;
    Ok(())
}
