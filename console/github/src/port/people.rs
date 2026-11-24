//! A person's sign-in, `Identity::sign_in_link` and `signed_in`, through
//! the factory's GitHub App's client for people (its OAuth client). A
//! security change of its own: the client's secret and the person's token
//! go on curl's standard input, never on a command line, and a handle made
//! for a sign-in holds no runner of gh, so it can ask nothing as the
//! factory. The wiring gives the client; the adapter reads no file and no
//! variable. A site given none answers `Refused(Unsupported)`.

use std::sync::Arc;

use crate::forge::uri;
use crate::GitHub;
use tehas_core::access::{self, Curl};
use tehas_core::accounts;
use tehas_ports::{
    Account, AccountId, AccountKind, Got, Login, Outcome, Passing, Realm, Refusal, Secret, Sent,
    SignInCode, SignInState, WebLink,
};

/// GitHub's web host: where a person signs in and trades the code.
const WEB: &str = "https://github.com";
/// GitHub's REST API, where none is given.
const API: &str = "https://api.github.com";

/// The factory's client for people, as the wiring reads it.
#[derive(Clone)]
pub struct PeopleClient {
    /// The client's id.
    pub id: String,
    /// The client's secret.
    pub secret: Secret,
    /// The REST API's address, `https://api.github.com` when None: a test's
    /// GitHub, as `TEHAS_GITHUB_API` names it.
    pub api: Option<String>,
    /// Where GitHub sends the person back, with the code and the state.
    pub callback: WebLink,
}

/// The client and what trades a code for a person: curl, or a test's.
#[derive(Clone)]
pub(crate) struct People {
    client: PeopleClient,
    trade: Arc<dyn access::GitHub>,
}

impl People {
    /// The client, trading through curl.
    pub(crate) fn new(client: PeopleClient, repo: &str) -> People {
        let trade = Curl {
            api: client.api.clone().unwrap_or_else(|| API.to_string()),
            web: WEB.to_string(),
            repo: repo.to_string(),
            client_id: client.id.clone(),
            client_secret: client.secret.expose().to_string(),
            console: Default::default(),
        };
        People::trading(client, Arc::new(trade))
    }

    /// The client, trading through `trade`.
    pub(crate) fn trading(client: PeopleClient, trade: Arc<dyn access::GitHub>) -> People {
        People { client, trade }
    }
}

impl GitHub {
    /// GitHub's page that asks the person to let the App know who they
    /// are, carrying `state`. No request. Sign-up is not offered: a person
    /// with no account has no standing to sign in for.
    pub(super) fn sign_in_page(&self, state: &SignInState) -> Got<WebLink> {
        let people = self.people()?;
        let link = format!(
            "{WEB}/login/oauth/authorize?client_id={}&redirect_uri={}&state={}&allow_signup=false",
            uri(&people.client.id),
            uri(people.client.callback.as_str()),
            uri(state.as_str()),
        );
        WebLink::new(link).map_err(|_| Outcome::Refused(Refusal::Invalid))
    }

    /// The code traded for the person's token, then who the token is: a
    /// person's account, by its id. A code GitHub does not take is
    /// `Forbidden`; a person it cannot read is `Unavailable`, nothing sent.
    pub(super) fn person_of(&mut self, code: &SignInCode) -> Got<(Account, Secret)> {
        let people = self.people()?.clone();
        if code.as_str().is_empty() {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let token = people
            .trade
            .token(code.as_str())
            .map_err(|_| Outcome::Forbidden)?;
        let person = people
            .trade
            .user(&token)
            .map_err(|_| Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::No,
            })?;
        let mut account = Account::new(
            Realm::CodeHost,
            Login::new(accounts::folded(&person.login)),
            AccountKind::Person,
        );
        account.id = Some(AccountId::new(person.id.to_string()));
        Ok((account, Secret::new(token)))
    }

    fn people(&self) -> Got<&People> {
        self.people
            .as_deref()
            .ok_or(Outcome::Refused(Refusal::Unsupported))
    }
}
