use clap::Parser;

use hypermarket_clob::persistence::snapshot::SnapshotStore;

#[derive(Parser, Debug)]
#[command(name = "snapshot_inspect")]
struct Args {
    #[arg(long)]
    snapshot: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let snapshot = SnapshotStore::load(std::path::Path::new(&args.snapshot))?
        .ok_or_else(|| anyhow::anyhow!("snapshot not found"))?;
    println!("version={}", snapshot.meta.version);
    println!("shard_id={}", snapshot.meta.shard_id);
    println!("last_seq={}", snapshot.meta.last_seq);
    println!("checksum={}", snapshot.meta.checksum);
    Ok(())
}
