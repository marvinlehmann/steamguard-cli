use crate::token::Jwt;
use crate::transport::{Transport, TransportError};

use super::{ApiRequest, ApiResponse, BuildableRequest};

use crate::protobufs::custom::CTwoFactor_Time_Request;
use crate::protobufs::service_twofactor::*;

const SERVICE_NAME: &str = "ITwoFactorService";

#[derive(Debug)]
pub struct TwoFactorClient<T>
where
	T: Transport,
{
	transport: T,
}

impl<T> TwoFactorClient<T>
where
	T: Transport,
{
	#[must_use]
	pub fn new(transport: T) -> Self {
		Self { transport }
	}

	pub fn add_authenticator(
		&self,
		req: CTwoFactor_AddAuthenticator_Request,
		access_token: &Jwt,
	) -> anyhow::Result<ApiResponse<CTwoFactor_AddAuthenticator_Response>> {
		let req = ApiRequest::new(SERVICE_NAME, "AddAuthenticator", 1, req)
			.with_access_token(access_token);
		let resp = self
			.transport
			.send_request::<CTwoFactor_AddAuthenticator_Request, CTwoFactor_AddAuthenticator_Response>(
				req,
			)?;
		Ok(resp)
	}

	pub fn finalize_authenticator(
		&self,
		req: CTwoFactor_FinalizeAddAuthenticator_Request,
		access_token: &Jwt,
	) -> anyhow::Result<ApiResponse<CTwoFactor_FinalizeAddAuthenticator_Response>> {
		let req = ApiRequest::new(SERVICE_NAME, "FinalizeAddAuthenticator", 1, req)
			.with_access_token(access_token);
		let resp = self
			.transport
			.send_request::<CTwoFactor_FinalizeAddAuthenticator_Request, CTwoFactor_FinalizeAddAuthenticator_Response>(
				req,
			)?;
		Ok(resp)
	}

	pub fn remove_authenticator(
		&self,
		req: CTwoFactor_RemoveAuthenticator_Request,
		access_token: &Jwt,
	) -> Result<ApiResponse<CTwoFactor_RemoveAuthenticator_Response>, TransportError> {
		let req = ApiRequest::new(SERVICE_NAME, "RemoveAuthenticator", 1, req)
			.with_access_token(access_token);
		let resp = self
			.transport
			.send_request::<CTwoFactor_RemoveAuthenticator_Request, CTwoFactor_RemoveAuthenticator_Response>(
				req,
			)?;
		Ok(resp)
	}

	pub fn query_status(
		&self,
		req: CTwoFactor_Status_Request,
		access_token: &Jwt,
	) -> anyhow::Result<ApiResponse<CTwoFactor_Status_Response>> {
		let req =
			ApiRequest::new(SERVICE_NAME, "QueryStatus", 1, req).with_access_token(access_token);
		let resp = self
			.transport
			.send_request::<CTwoFactor_Status_Request, CTwoFactor_Status_Response>(req)?;
		Ok(resp)
	}

	pub fn query_time(&self) -> anyhow::Result<ApiResponse<CTwoFactor_Time_Response>> {
		let req = ApiRequest::new(SERVICE_NAME, "QueryTime", 1, CTwoFactor_Time_Request::new());
		let resp = self
			.transport
			.send_request::<CTwoFactor_Time_Request, CTwoFactor_Time_Response>(req)?;
		Ok(resp)
	}
}

macro_rules! impl_buildable_req {
	($type:ty, $needs_auth:literal) => {
		impl BuildableRequest for $type {
			fn method() -> reqwest::Method {
				reqwest::Method::POST
			}

			fn requires_access_token() -> bool {
				$needs_auth
			}
		}
	};
}

impl_buildable_req!(CTwoFactor_AddAuthenticator_Request, true);
impl_buildable_req!(CTwoFactor_FinalizeAddAuthenticator_Request, true);
impl_buildable_req!(CTwoFactor_RemoveAuthenticator_Request, true);
impl_buildable_req!(CTwoFactor_Status_Request, true);
impl_buildable_req!(CTwoFactor_Time_Request, false);
