use bottles_core::{Library, LibraryItem};
use clap::Subcommand;

use crate::{Result, missing, run_operation};

#[derive(Subcommand)]
pub enum Command {
    /// List entries from every library provider.
    List,
    /// Match entry titles without case sensitivity.
    Search { query: String },
    /// Launch an entry by provider and entry ID.
    Launch { provider: String, entry: String },
}

pub async fn run(library: &Library, command: Command) -> Result<()> {
    let items = library.list().await?;
    match command {
        Command::List => items.iter().for_each(print),
        Command::Search { query } => {
            let query = query.trim().to_lowercase();
            items
                .iter()
                .filter(|item| item.entry().title.to_lowercase().contains(&query))
                .for_each(print);
        }
        Command::Launch { provider, entry } => {
            let item = items
                .into_iter()
                .find(|item| item.provider_id() == provider && item.entry().id == entry)
                .ok_or_else(|| missing("library entry", format!("{provider}/{entry}")))?;
            run_operation(item.launch()?).await?;
        }
    }
    Ok(())
}

fn print(item: &LibraryItem) {
    println!(
        "{}\t{}\t{}",
        item.provider_id(),
        item.entry().id,
        item.entry().title
    );
}
