use cnut::prelude::*;
use cnut::tokio;

#[tokio::main]
async fn main() -> cnut::error::Result<()> {
    start_logger();

    let artifacts = Artifacts::builder()
        .local_path("../casper-node")
        .build()
        .await?;

    Network::new()
        .with(5 * Node::validator(artifacts.clone()).name("Alice"))
        .with(Node::validator(artifacts.clone()).name("Bob"))
        .with(Node::validator(artifacts.clone()))
        //.with(5 * Node::validator(artifacts.clone()).config("../config.toml"))
        //.with(15 * Node::keep_up(artifacts.clone()))
        //.with(Chainspec::from(artifacts))
        .run()
        .await?;

    Ok(())
}

pub fn start_logger() {
    use flexi_logger::{
        filter::{LogLineFilter, LogLineWriter},
        DeferredNow, LogSpecification, Logger,
    };

    pub struct OurCrateOnly;

    impl LogLineFilter for OurCrateOnly {
        fn write(
            &self,
            now: &mut DeferredNow,
            record: &log::Record,
            log_line_writer: &dyn LogLineWriter,
        ) -> std::io::Result<()> {
            if record
                .module_path()
                .map_or(true, |path| path.starts_with("cnut"))
            {
                log_line_writer.write(now, record)?;
            }
            Ok(())
        }
    }

    Logger::with(LogSpecification::debug())
        .filter(Box::new(OurCrateOnly))
        .start()
        .expect("Failed to start the logger");
}
