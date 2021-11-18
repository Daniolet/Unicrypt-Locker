use anchor_client::{
    solana_sdk::{
        commitment_config::CommitmentConfig, pubkey::Pubkey, signature::read_keypair_file,
    },
    Client,
};
use anyhow::{anyhow, Result};

use country_list::CountryBanList;
use solana_sdk::{signature::Keypair, signer::Signer};
use structopt::StructOpt;

#[derive(Debug)]
struct CliKeypair<A> {
    path: String,
    ty: std::marker::PhantomData<A>,
}

impl<A> std::fmt::Display for CliKeypair<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.path)
    }
}

impl<A> std::str::FromStr for CliKeypair<A> {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            path: s.to_string(),
            ty: std::marker::PhantomData {},
        })
    }
}

impl<A> AsRef<String> for CliKeypair<A> {
    fn as_ref(&self) -> &String {
        &self.path
    }
}

impl<A> Default for CliKeypair<A>
where
    A: DefaultPath,
{
    fn default() -> Self {
        Self {
            path: A::default_path(),
            ty: std::marker::PhantomData {},
        }
    }
}

trait DefaultPath {
    fn default_path() -> String;
}

#[derive(Debug)]
struct Payer;

impl DefaultPath for Payer {
    fn default_path() -> String {
        shellexpand::tilde("~/.config/solana/id.json").to_string()
    }
}

#[derive(Debug, StructOpt)]
struct Opts {
    #[structopt(long)]
    program_id: Pubkey,
    #[structopt(long)]
    cluster: anchor_client::Cluster,
    #[structopt(long, default_value)]
    payer: CliKeypair<Payer>,
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Initialize country list.
    Init {
        #[structopt(long)]
        countries: String,
    },
    /// Show all countries data or data for specific one.
    Show {
        #[structopt(long)]
        banlist: Pubkey,
        #[structopt(long)]
        country: Option<String>,
    },
    Flip {
        #[structopt(long)]
        banlist: Pubkey,
        #[structopt(long)]
        country: String,
        #[structopt(long)]
        ban: bool,
    },
}

fn main() -> Result<()> {
    let opts = Opts::from_args();

    let payer = read_keypair_file(opts.payer.as_ref())
        .map_err(|err| anyhow!("failed to read keypair: {}", err))?;
    let payer_copy = read_keypair_file(opts.payer.as_ref())
        .map_err(|err| anyhow!("failed to read keypair: {}", err))?;

    let client = Client::new_with_options(opts.cluster, payer, CommitmentConfig::processed());
    let client = client.program(opts.program_id);

    let countries_list = Keypair::new();

    match opts.cmd {
        Command::Init { countries } => {
            let file = std::fs::read(countries)?;
            let mut rdr = csv::Reader::from_reader(&*file);
            let mut countries = Vec::new();
            for result in rdr.records() {
                let record = result?;
                let country_code = record.get(2).unwrap();
                let country_code = if country_code.is_empty() {
                    "UN".to_string()
                } else {
                    country_code.to_string()
                };
                let code_bytes = country_list::string_to_byte_array(&country_code);
                countries.push(code_bytes);
            }
            countries.sort();
            countries.dedup();

            let r = client
                .request()
                .accounts(country_list::accounts::Initialize {
                    country_banlist: countries_list.pubkey(),
                    admin: client.payer(),
                    system_program: anchor_client::solana_sdk::system_program::id(),
                })
                .args(country_list::instruction::Initialize { countries })
                .signer(&payer_copy)
                .signer(&countries_list)
                .send()?;

            println!("Result:\n{}", r);
            println!("Countries Banlist Address: {}", countries_list.pubkey());
        }
        Command::Show { banlist, country } => {
            let banlist: CountryBanList = client.account(banlist)?;

            match country {
                Some(country) => {
                    let bytes = country_list::string_to_byte_array(&country);
                    match banlist.countries.iter().find(|c| c.code == bytes) {
                        Some(country) => {
                            println!("{:#?}", country);
                        }
                        None => {
                            println!("Unknown country: {}", country);
                        }
                    }
                }
                None => {
                    println!("{:#?}", banlist);
                }
            }
        }
        Command::Flip {
            banlist,
            country,
            ban,
        } => {
            let r = client
                .request()
                .accounts(country_list::accounts::FlipBan {
                    country_banlist: banlist,
                    admin: client.payer(),
                })
                .args(country_list::instruction::FlipBan {
                    country,
                    value: ban,
                })
                .signer(&payer_copy)
                .send()?;

            println!("Result:\n{}", r);
        }
    }

    Ok(())
}
