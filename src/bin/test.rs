use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (tx,mut rx) = mpsc::channel(1);
    tx.try_send(1)?;

    let res = rx.try_recv();
    println!("{:?}",res);
    Ok(())
}
