use serde::Deserialize;
use std::env::var;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub network_name: String,
    pub db_username: String,
    pub db_password: String,
    pub proxy_label: String,
    pub domain: String,
    pub certs: String,
}

impl Config {
    pub fn new<T: Into<String>>(
        network_name: Option<T>,
        db_username: Option<T>,
        db_password: Option<T>,
        proxy_label: Option<T>,
        domain: Option<T>,
        certs: Option<T>,
    ) -> Self {
        Self {
            network_name: network_name
                .map(|t| t.into())
                .unwrap_or_else(|| Self::default().network_name),
            db_username: db_username
                .map(|t| t.into())
                .unwrap_or_else(|| Self::default().db_username),
            db_password: db_password
                .map(|t| t.into())
                .unwrap_or_else(|| Self::default().db_password),
            proxy_label: proxy_label
                .map(|t| t.into())
                .unwrap_or_else(|| Self::default().proxy_label),
            domain: domain
                .map(|t| t.into())
                .unwrap_or_else(|| Self::default().domain),
            certs: certs
                .map(|t| t.into())
                .unwrap_or_else(|| Self::default().certs),
        }
    }

    pub fn from_envs() -> Self {
        Self::new(
            var("MAKEPRESS_NETWORK").ok(),
            var("MAKEPRESS_DB_USERNAME").ok(),
            var("MAKEPRESS_DB_PASSWORD").ok(),
            var("MAKEPRESS_PROXY_LABEL").ok(),
            var("MAKEPRESS_DOMAIN").ok(),
            var("MAKEPRESS_CERTS").ok(),
        )
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            network_name: "prometheus.makepress.network".to_string(),
            db_username: "makepress".to_string(),
            db_password: "makepress".to_string(),
            proxy_label: "prometheus.makepress.proxy".to_string(),
            domain: "prometheus.makepress".to_string(),
            certs: "/etc/nginx/certs".to_string(),
        }
    }
}
