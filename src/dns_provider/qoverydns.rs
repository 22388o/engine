use std::net::Ipv4Addr;
use tera::Context as TeraContext;

use crate::dns_provider::errors::DnsProviderError;
use crate::dns_provider::{DnsProvider, DnsProviderConfiguration, Kind};
use crate::io_models::{Context, Domain};

pub struct QoveryDnsConfig {
    api_url: String,
    api_port: String,
    api_key: String,
}

pub struct QoveryDns {
    context: Context,
    id: String,
    api_url: String,
    api_port: String,
    api_key: String,
    secret_name: String,
    name: String,
    domain: Domain,
}

impl QoveryDns {
    pub fn new(
        context: Context,
        id: &str,
        api_url: &str,
        api_port: &str,
        api_key: &str,
        secret_name: &str,
        name: &str,
        domain: Domain,
    ) -> Self {
        QoveryDns {
            context,
            id: id.to_string(),
            api_url: api_url.to_string(),
            api_port: api_port.to_string(),
            api_key: api_key.to_string(),
            secret_name: secret_name.to_string(),
            name: name.to_string(),
            domain,
        }
    }
}

impl DnsProvider for QoveryDns {
    fn context(&self) -> &Context {
        &self.context
    }

    fn provider_name(&self) -> &str {
        "qoverydns"
    }

    fn kind(&self) -> Kind {
        Kind::QoveryDns
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn insert_into_teracontext<'a>(&self, context: &'a mut TeraContext) -> &'a mut TeraContext {
        context
    }

    fn provider_configuration(&self) -> DnsProviderConfiguration {
        DnsProviderConfiguration::QoveryDns(QoveryDnsConfig {
            api_url: self.api_url.clone(),
            api_port: self.api_port.clone(),
            api_key: self.api_key.clone(),
        })
    }

    fn domain(&self) -> &Domain {
        &self.domain
    }

    fn resolvers(&self) -> Vec<Ipv4Addr> {
        vec![Ipv4Addr::new(8, 8, 8, 8), Ipv4Addr::new(8, 8, 4, 4)]
    }

    fn is_valid(&self) -> Result<(), DnsProviderError> {
        if self.api_key.is_empty() || self.api_port.is_empty() || self.api_key.is_empty() {
            Err(DnsProviderError::InvalidCredentials)
        } else {
            Ok(())
        }
    }
}
