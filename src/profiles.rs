use std::{
    io::{self, Write},
    sync::Arc,
};

use async_trait::async_trait;
use bottles_core::{AccountLink, AccountLinkInteraction, Profile, Profiles};
use clap::{Args, Subcommand};
use url::Url;
use uuid::Uuid;

use crate::{Result, missing, run_operation};

#[derive(Subcommand)]
pub enum Command {
    /// List profiles and their linked accounts.
    List,
    /// Show the selected profile or one chosen by UUID or name.
    Show { profile: Option<String> },
    /// List account providers available through plugins.
    Providers,
    /// Create and select a new profile.
    Create { name: String },
    /// Select a profile by UUID or name.
    Select { profile: String },
    /// Rename a profile.
    Rename { profile: String, name: String },
    /// Delete a profile.
    Delete { profile: String },
    /// Link a provider account to a profile.
    Link(AccountArgs),
    /// Unlink an account by link UUID.
    Unlink { link: Uuid },
}

#[derive(Args)]
pub struct AccountArgs {
    provider: String,
    #[arg(long)]
    profile: Option<String>,
}

pub async fn run(profiles: &Profiles, command: Command) -> Result<()> {
    match command {
        Command::List => {
            let selected = profiles.selected().id();
            for profile in profiles.list() {
                print(&profile, profile.id() == selected);
            }
        }
        Command::Show { profile } => {
            let profile = find(profiles, profile.as_deref())?;
            print(&profile, profile.id() == profiles.selected().id());
        }
        Command::Providers => {
            for provider in profiles.account_providers() {
                println!("{}\t{}", provider.id, provider.name);
            }
        }
        Command::Create { name } => print(&profiles.create(name).await?, true),
        Command::Select { profile } => {
            let id = find(profiles, Some(&profile))?.id();
            print(&profiles.select(id).await?, true);
        }
        Command::Rename { profile, name } => {
            let id = find(profiles, Some(&profile))?.id();
            let profile = profiles.rename(id, name).await?;
            print(&profile, profile.id() == profiles.selected().id());
        }
        Command::Delete { profile } => {
            profiles
                .delete(find(profiles, Some(&profile))?.id())
                .await?
        }
        Command::Link(args) => {
            let profile = find(profiles, args.profile.as_deref())?;
            let account = run_operation(profiles.link_account(
                profile.id(),
                args.provider,
                Arc::new(TerminalInput),
            ))
            .await?;
            print_account(&account);
        }
        Command::Unlink { link } => profiles.unlink_account(link).await?,
    }
    Ok(())
}

fn find(profiles: &Profiles, selector: Option<&str>) -> Result<Profile> {
    let Some(selector) = selector else {
        return Ok(profiles.selected());
    };
    if let Ok(id) = Uuid::parse_str(selector) {
        return profiles
            .list()
            .into_iter()
            .find(|profile| profile.id() == id)
            .ok_or_else(|| missing("profile", selector).into());
    }
    let mut names = profiles
        .list()
        .into_iter()
        .filter(|profile| profile.name() == selector);
    let first = names.next().ok_or_else(|| missing("profile", selector))?;
    if names.next().is_some() {
        return Err(format!("ambiguous profile name: {selector}; use its UUID").into());
    }
    Ok(first)
}

fn print(profile: &Profile, selected: bool) {
    println!(
        "{}\t{}{}",
        profile.id(),
        profile.name(),
        if selected { "\tselected" } else { "" }
    );
    for account in profile.accounts() {
        print_account(account);
    }
}

fn print_account(account: &AccountLink) {
    println!(
        "account\t{}\t{}\t{}\t{}\t{}",
        account.link_id,
        account.provider.id,
        account.provider.name,
        account.identity.account_id,
        account.identity.display_name
    );
}

struct TerminalInput;

#[async_trait]
impl AccountLinkInteraction for TerminalInput {
    async fn request_input(
        &self,
        url: Url,
        instructions: String,
    ) -> std::result::Result<String, String> {
        println!("{instructions}\n{url}");
        print!("Authorization code: ");
        io::stdout().flush().map_err(|error| error.to_string())?;
        // A terminal read may outlive cancellation; do not block the async runtime on it.
        let (send, receive) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let mut input = String::new();
            let result = io::stdin().read_line(&mut input).map(|len| (len, input));
            let _ = send.send(result);
        });
        let (len, input) = receive
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        if len == 0 {
            return Err("standard input closed".into());
        }
        Ok(input.trim().to_owned())
    }
}
