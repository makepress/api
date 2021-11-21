use std::collections::HashMap;

use bollard::{
    container::{Config, CreateContainerOptions, ListContainersOptions},
    models::{ContainerSummaryInner, HostConfig, Ipam, PortBinding},
    network::{CreateNetworkOptions, ListNetworksOptions},
    Docker,
};
use serde::Serialize;

use crate::{config::Config as Conf, const_expr_count, hash_map, CONTAINER_LABEL};

type Result<T> = std::result::Result<T, bollard::errors::Error>;

#[derive(Debug, Clone, Serialize)]
pub struct ListContainersResponse {
    //#[serde(skip_serializing_if = "Option::is_none")]
    //db_state: Option<ContainerSummaryInner>,
    containers: Vec<Summary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    name: String,
    status: String,
    state: String,
    created: i64,
}

impl From<ContainerSummaryInner> for Summary {
    fn from(c: ContainerSummaryInner) -> Self {
        Self {
            name: c
                .labels
                .unwrap_or_default()
                .get("prometheus.makepress.name")
                .unwrap_or(&"ERROR 404".to_string())
                .to_string(),
            status: c.status.map_or("".to_string(), |n| n),
            state: c.state.map_or("".to_string(), |n| n),
            created: c.created.map_or(0, |n| n),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContainerManager {
    docker_instance: Docker,
    config: Conf,
}

impl From<Docker> for ContainerManager {
    fn from(d: Docker) -> Self {
        Self {
            docker_instance: d,
            config: Conf::from_envs(),
        }
    }
}

impl ContainerManager {
    pub async fn init(&self) -> Result<()> {
        // Check network exsists and create it if it doesn't
        println!("Checking network exists...");
        let network_exists = self.network_exists().await?;
        if !network_exists {
            println!("Creating network...");
            self.create_network(&self.config.network_name).await?;
        }

        // Check proxy container exists
        let proxy = self.get_proxy().await?;
        match proxy {
            Some(p) if p.state != Some("running".to_string()) => {
                self.docker_instance
                    .start_container::<&str>(&p.names.as_ref().unwrap()[0], None)
                    .await?;
            }
            None => {
                self.docker_instance
                    .create_container(
                        Some(CreateContainerOptions {
                            name: "makepress-proxy",
                        }),
                        Config {
                            exposed_ports: Some(hash_map! {
                                "80/tcp" => HashMap::<(), ()>::new(),
                                "443/tcp" => HashMap::<(), ()>::new(),
                            }),
                            image: Some("nginxproxy/nginx-proxy"),
                            labels: Some(hash_map! {&self.config.proxy_label as &str => ""}),
                            host_config: Some(HostConfig {
                                binds: Some(vec![
                                    "/var/run/docker.sock:/tmp/docker.sock:ro".to_string(),
                                    format!("{}:/etc/nginx/certs", self.config.certs),
                                ]),
                                port_bindings: Some(hash_map! {
                                    "80/tcp".to_string() => Some(vec![PortBinding {
                                        host_port: Some("80".to_string()),
                                        ..Default::default()
                                    }]),
                                    "443/tcp".to_string() => Some(vec![PortBinding {
                                        host_port: Some("443".to_string()),
                                        ..Default::default()
                                    }])
                                }),
                                network_mode: Some(self.config.network_name.clone()),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    )
                    .await?;
                self.docker_instance
                    .start_container::<&str>("makepress-proxy", None)
                    .await?;
            }
            _ => {}
        };
        Ok(())
    }

    async fn network_exists(&self) -> Result<bool> {
        Ok(!self
            .docker_instance
            .list_networks(Some(ListNetworksOptions {
                filters: hash_map! {
                    "label" => vec![&self.config.network_name as &str],
                    "name" => vec![&self.config.network_name as &str]
                },
            }))
            .await?
            .is_empty())
    }

    async fn create_network<T: AsRef<str>>(&self, network_name: T) -> Result<()> {
        self.docker_instance
            .create_network(CreateNetworkOptions {
                name: network_name.as_ref(),
                check_duplicate: true,
                driver: "bridge",
                internal: false,
                attachable: false,
                ingress: false,
                ipam: Ipam {
                    ..Default::default()
                },
                enable_ipv6: false,
                options: HashMap::new(),
                labels: hash_map! {network_name.as_ref() => ""},
            })
            .await?;
        Ok(())
    }

    async fn get_proxy(&self) -> Result<Option<ContainerSummaryInner>> {
        Ok(self
            .docker_instance
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters: hash_map! {
                    "label" => vec![&self.config.proxy_label as &str]
                },
                ..Default::default()
            }))
            .await?
            .get(0)
            .cloned())
    }

    pub async fn list_containers(&self) -> Result<ListContainersResponse> {
        let containers = self
            .docker_instance
            .list_containers(Some(ListContainersOptions::<String> {
                all: true,
                filters: hash_map! {
                    "label".to_string() => vec![CONTAINER_LABEL.to_string()]
                },
                ..Default::default()
            }))
            .await?
            .into_iter()
            .map(|s| s.into())
            .collect();
        Ok(ListContainersResponse { containers })
    }
}
