use std::borrow::Cow;

use hmacsha1::hmac_sha1;
use log::*;
use reqwest::{
	cookie::CookieStore,
	header::{CONTENT_TYPE, COOKIE, USER_AGENT},
	Url,
};
use secrecy::ExposeSecret;
use serde::Deserialize;

use crate::{
	steamapi::{self},
	transport::Transport,
	SteamGuardAccount,
};

lazy_static! {
	static ref STEAM_COOKIE_URL: Url = "https://steamcommunity.com".parse::<Url>().unwrap();
}

/// Provides an interface that wraps the Steam mobile confirmation API.
///
/// Only compatible with WebApiTransport.
pub struct Confirmer<'a, T> {
	account: &'a SteamGuardAccount,
	transport: T,
}

impl<'a, T> Confirmer<'a, T>
where
	T: Transport + Clone,
{
	pub fn new(transport: T, account: &'a SteamGuardAccount) -> Self {
		Self { account, transport }
	}

	fn get_confirmation_query_params<'q>(
		&'q self,
		tag: &'q str,
		time: u64,
	) -> Vec<(&'static str, Cow<'q, str>)> {
		[
			("p", self.account.device_id.as_str().into()),
			("a", self.account.steam_id.to_string().into()),
			(
				"k",
				generate_confirmation_hash_for_time(
					time,
					tag,
					self.account.identity_secret.expose_secret(),
				)
				.into(),
			),
			("t", time.to_string().into()),
			("m", "react".into()),
			("tag", tag.into()),
		]
		.into()
	}

	fn build_cookie_jar(&self) -> reqwest::cookie::Jar {
		let cookies = reqwest::cookie::Jar::default();
		let tokens = self.account.tokens.as_ref().unwrap();
		cookies.add_cookie_str("dob=", &STEAM_COOKIE_URL);
		cookies.add_cookie_str(
			format!("steamid={}", self.account.steam_id).as_str(),
			&STEAM_COOKIE_URL,
		);
		cookies.add_cookie_str(
			format!(
				"steamLoginSecure={}||{}",
				self.account.steam_id,
				tokens.access_token().expose_secret()
			)
			.as_str(),
			&STEAM_COOKIE_URL,
		);
		cookies
	}

	pub fn get_trade_confirmations(&self) -> Result<Vec<Confirmation>, ConfirmerError> {
		let cookies = self.build_cookie_jar();
		let client = self.transport.innner_http_client()?;

		let time = steamapi::get_server_time(self.transport.clone())?.server_time();
		let resp = client
			.get(
				"https://steamcommunity.com/mobileconf/getlist"
					.parse::<Url>()
					.unwrap(),
			)
			.header(USER_AGENT, "steamguard-cli")
			.header(COOKIE, cookies.cookies(&STEAM_COOKIE_URL).unwrap())
			.query(&self.get_confirmation_query_params("conf", time))
			.send()?;

		trace!("{:?}", resp);
		let text = resp.text().unwrap();
		debug!("Confirmations response: {}", text);

		let mut deser = serde_json::Deserializer::from_str(text.as_str());
		let body: ConfirmationListResponse = serde_path_to_error::deserialize(&mut deser)?;

		if body.needauth.unwrap_or(false) {
			return Err(ConfirmerError::InvalidTokens);
		}
		if !body.success {
			return Err(anyhow!("Server responded with failure.").into());
		}
		Ok(body.conf)
	}

	/// Respond to a confirmation.
	///
	/// Host: https://steamcommunity.com
	/// Steam Endpoint: `GET /mobileconf/ajaxop`
	fn send_confirmation_ajax(
		&self,
		conf: &Confirmation,
		action: ConfirmationAction,
	) -> Result<(), ConfirmerError> {
		debug!("responding to a single confirmation: send_confirmation_ajax()");
		let operation = action.to_operation();

		let cookies = self.build_cookie_jar();
		let client = self.transport.innner_http_client()?;

		let time = steamapi::get_server_time(self.transport.clone())?.server_time();
		let mut query_params = self.get_confirmation_query_params("conf", time);
		query_params.push(("op", operation.into()));
		query_params.push(("cid", Cow::Borrowed(&conf.id)));
		query_params.push(("ck", Cow::Borrowed(&conf.nonce)));

		let resp = client
			.get(
				"https://steamcommunity.com/mobileconf/ajaxop"
					.parse::<Url>()
					.unwrap(),
			)
			.header(USER_AGENT, "steamguard-cli")
			.header(COOKIE, cookies.cookies(&STEAM_COOKIE_URL).unwrap())
			.query(&query_params)
			.send()?;

		trace!("send_confirmation_ajax() response: {:?}", &resp);
		debug!(
			"send_confirmation_ajax() response status code: {}",
			&resp.status()
		);

		let raw = resp.text()?;
		debug!("send_confirmation_ajax() response body: {:?}", &raw);

		let mut deser = serde_json::Deserializer::from_str(raw.as_str());
		let body: SendConfirmationResponse = serde_path_to_error::deserialize(&mut deser)?;

		if body.needsauth.unwrap_or(false) {
			return Err(ConfirmerError::InvalidTokens);
		}
		if !body.success {
			return Err(anyhow!("Server responded with failure.").into());
		}

		Ok(())
	}

	pub fn accept_confirmation(&self, conf: &Confirmation) -> Result<(), ConfirmerError> {
		self.send_confirmation_ajax(conf, ConfirmationAction::Accept)
	}

	pub fn deny_confirmation(&self, conf: &Confirmation) -> Result<(), ConfirmerError> {
		self.send_confirmation_ajax(conf, ConfirmationAction::Deny)
	}

	/// Respond to more than 1 confirmation.
	///
	/// Host: https://steamcommunity.com
	/// Steam Endpoint: `GET /mobileconf/multiajaxop`
	fn send_multi_confirmation_ajax(
		&self,
		confs: &[Confirmation],
		action: ConfirmationAction,
	) -> Result<(), ConfirmerError> {
		debug!("responding to bulk confirmations: send_multi_confirmation_ajax()");
		if confs.is_empty() {
			debug!("confs is empty, nothing to do.");
			return Ok(());
		}
		let operation = action.to_operation();

		let cookies = self.build_cookie_jar();
		let client = self.transport.innner_http_client()?;

		let time = steamapi::get_server_time(self.transport.clone())?.server_time();
		let mut query_params = self.get_confirmation_query_params("conf", time);
		query_params.push(("op", operation.into()));
		for conf in confs.iter() {
			query_params.push(("cid[]", Cow::Borrowed(&conf.id)));
			query_params.push(("ck[]", Cow::Borrowed(&conf.nonce)));
		}
		let query_params = self.build_multi_conf_query_string(&query_params);
		// despite being called query parameters, they will actually go in the body
		debug!("query_params: {}", &query_params);

		let resp = client
			.post(
				"https://steamcommunity.com/mobileconf/multiajaxop"
					.parse::<Url>()
					.unwrap(),
			)
			.header(USER_AGENT, "steamguard-cli")
			.header(COOKIE, cookies.cookies(&STEAM_COOKIE_URL).unwrap())
			.header(
				CONTENT_TYPE,
				"application/x-www-form-urlencoded; charset=UTF-8",
			)
			.body(query_params)
			.send()?;

		trace!("send_multi_confirmation_ajax() response: {:?}", &resp);
		debug!(
			"send_multi_confirmation_ajax() response status code: {}",
			&resp.status()
		);

		let raw = resp.text()?;
		debug!("send_multi_confirmation_ajax() response body: {:?}", &raw);

		let mut deser = serde_json::Deserializer::from_str(raw.as_str());
		let body: SendConfirmationResponse = serde_path_to_error::deserialize(&mut deser)?;

		if body.needsauth.unwrap_or(false) {
			return Err(ConfirmerError::InvalidTokens);
		}
		if !body.success {
			return Err(anyhow!("Server responded with failure.").into());
		}

		Ok(())
	}

	pub fn accept_confirmations(&self, confs: &[Confirmation]) -> Result<(), ConfirmerError> {
		self.send_multi_confirmation_ajax(confs, ConfirmationAction::Accept)
	}

	pub fn deny_confirmations(&self, confs: &[Confirmation]) -> Result<(), ConfirmerError> {
		self.send_multi_confirmation_ajax(confs, ConfirmationAction::Deny)
	}

	fn build_multi_conf_query_string(&self, params: &[(&str, Cow<str>)]) -> String {
		params
			.iter()
			.map(|(k, v)| format!("{}={}", k, v))
			.collect::<Vec<_>>()
			.join("&")
	}

	/// Steam Endpoint: `GET /mobileconf/details/:id`
	pub fn get_confirmation_details(&self, conf: &Confirmation) -> anyhow::Result<String> {
		#[derive(Debug, Clone, Deserialize)]
		struct ConfirmationDetailsResponse {
			pub success: bool,
			pub html: String,
		}

		let cookies = self.build_cookie_jar();
		let client = self.transport.innner_http_client()?;

		let time = steamapi::get_server_time(self.transport.clone())?.server_time();
		let query_params = self.get_confirmation_query_params("details", time);

		let resp = client
			.get(
				format!("https://steamcommunity.com/mobileconf/details/{}", conf.id)
					.parse::<Url>()
					.unwrap(),
			)
			.header(USER_AGENT, "steamguard-cli")
			.header(COOKIE, cookies.cookies(&STEAM_COOKIE_URL).unwrap())
			.query(&query_params)
			.send()?;

		let text = resp.text()?;
		let mut deser = serde_json::Deserializer::from_str(text.as_str());
		let body: ConfirmationDetailsResponse = serde_path_to_error::deserialize(&mut deser)?;

		ensure!(body.success);
		Ok(body.html)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationAction {
	Accept,
	Deny,
}

impl ConfirmationAction {
	fn to_operation(self) -> &'static str {
		match self {
			ConfirmationAction::Accept => "allow",
			ConfirmationAction::Deny => "cancel",
		}
	}
}

#[derive(Debug, thiserror::Error)]
pub enum ConfirmerError {
	#[error("Invalid tokens, login or token refresh required.")]
	InvalidTokens,
	#[error("Network failure: {0}")]
	NetworkFailure(#[from] reqwest::Error),
	#[error("Failed to deserialize response: {0}")]
	DeserializeError(#[from] serde_path_to_error::Error<serde_json::Error>),
	#[error("Unknown error: {0}")]
	Unknown(#[from] anyhow::Error),
}

/// A mobile confirmation. There are multiple things that can be confirmed, like trade offers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Confirmation {
	#[serde(rename = "type")]
	pub conf_type: ConfirmationType,
	pub type_name: String,
	pub id: String,
	/// Trade offer ID or market transaction ID
	pub creator_id: String,
	pub nonce: String,
	pub creation_time: u64,
	pub cancel: String,
	pub accept: String,
	pub icon: Option<String>,
	pub multi: bool,
	pub headline: String,
	pub summary: Vec<String>,
}

impl Confirmation {
	/// Human readable representation of this confirmation.
	pub fn description(&self) -> String {
		format!(
			"{:?} - {} - {}",
			self.conf_type,
			self.headline,
			self.summary.join(", ")
		)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[repr(u32)]
#[serde(from = "u32")]
/// Source: <https://github.com/SteamDatabase/SteamTracking/blob/6e7797e69b714c59f4b5784780b24753c17732ba/Structs/enums.steamd#L1607-L1616>
pub enum ConfirmationType {
	Test = 1,
	Trade = 2,
	MarketSell = 3,
	FeatureOptOut = 4,
	PhoneNumberChange = 5,
	AccountRecovery = 6,
	Unknown(u32),
}

impl From<u32> for ConfirmationType {
	fn from(text: u32) -> Self {
		match text {
			1 => ConfirmationType::Test,
			2 => ConfirmationType::Trade,
			3 => ConfirmationType::MarketSell,
			4 => ConfirmationType::FeatureOptOut,
			5 => ConfirmationType::PhoneNumberChange,
			6 => ConfirmationType::AccountRecovery,
			v => ConfirmationType::Unknown(v),
		}
	}
}

#[derive(Debug, Deserialize)]
pub struct ConfirmationListResponse {
	pub success: bool,
	#[serde(default)]
	pub needauth: Option<bool>,
	#[serde(default)]
	pub conf: Vec<Confirmation>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct SendConfirmationResponse {
	pub success: bool,
	#[serde(default)]
	pub needsauth: Option<bool>,
}

fn build_time_bytes(time: u64) -> [u8; 8] {
	time.to_be_bytes()
}

fn generate_confirmation_hash_for_time(
	time: u64,
	tag: &str,
	identity_secret: impl AsRef<[u8]>,
) -> String {
	let decode: &[u8] = &base64::decode(identity_secret).unwrap();
	let time_bytes = build_time_bytes(time);
	let tag_bytes = tag.as_bytes();
	let array = [&time_bytes, tag_bytes].concat();
	let hash = hmac_sha1(decode, &array);
	base64::encode(hash)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_confirmations() -> anyhow::Result<()> {
		struct Test {
			text: &'static str,
			confirmation_type: ConfirmationType,
		}
		let cases = [
			Test {
				text: include_str!("fixtures/confirmations/email-change.json"),
				confirmation_type: ConfirmationType::AccountRecovery,
			},
			Test {
				text: include_str!("fixtures/confirmations/phone-number-change.json"),
				confirmation_type: ConfirmationType::PhoneNumberChange,
			},
		];
		for case in cases.iter() {
			let confirmations = serde_json::from_str::<ConfirmationListResponse>(case.text)?;

			assert_eq!(confirmations.conf.len(), 1);

			let confirmation = &confirmations.conf[0];

			assert_eq!(confirmation.conf_type, case.confirmation_type);
		}

		Ok(())
	}

	#[test]
	fn test_parse_confirmations_2() -> anyhow::Result<()> {
		struct Test {
			text: &'static str,
		}
		let cases = [Test {
			text: include_str!("fixtures/confirmations/need-auth.json"),
		}];
		for case in cases.iter() {
			let confirmations = serde_json::from_str::<ConfirmationListResponse>(case.text)?;

			assert_eq!(confirmations.conf.len(), 0);
			assert_eq!(confirmations.needauth, Some(true));
		}

		Ok(())
	}

	#[test]
	fn test_generate_confirmation_hash_for_time() {
		assert_eq!(
			generate_confirmation_hash_for_time(1617591917, "conf", "GQP46b73Ws7gr8GmZFR0sDuau5c="),
			String::from("NaL8EIMhfy/7vBounJ0CvpKbrPk=")
		);
	}
}
