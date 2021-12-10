use std::collections::HashMap;

use async_trait::async_trait;
use bollard::{
    container::{Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions},
    models::{ContainerSummaryInner, HostConfig, Ipam, PortBinding},
    network::{CreateNetworkOptions, ListNetworksOptions},
    Docker,
};
use makepress_lib::{Error, InstanceInfo, MakepressManager, Status};

use crate::{config::Config as Conf, const_expr_count, hash_map, CONTAINER_LABEL, DB_LABEL};

type Result<T> = std::result::Result<T, Error>;

macro_rules! flushed_print {
    ($($arg : tt) *) => {{
        use std::io::{self, Write};
        print!($($arg)*);
        io::stdout().flush().unwrap();
    }};
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

#[async_trait]
impl MakepressManager for ContainerManager {
    async fn get<T: AsRef<str> + Send>(&self, name: T) -> Result<InstanceInfo> {
        let label = format!("prometheus.makepress.name={}", name.as_ref());
        let wordpress = {
            let v = self
                .docker_instance
                .list_containers(Some(ListContainersOptions::<_> {
                    all: true,
                    filters: hash_map! {
                        "label" => vec![
                            CONTAINER_LABEL,
                            &label
                        ],
                    },
                    ..Default::default()
                }))
                .await
                .map_err::<Error, _>(|e| {
                    (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
                })?;

            v.get(0)
                .ok_or_else(|| Error::InstanceMissing(name.as_ref().to_string()))?
                .clone()
        };
        let database = {
            let v = self
                .docker_instance
                .list_containers(Some(ListContainersOptions::<_> {
                    all: true,
                    filters: hash_map! {
                        "label" => vec! [
                            DB_LABEL,
                            &label
                        ],
                    },
                    ..Default::default()
                }))
                .await
                .map_err::<Error, _>(|e| {
                    (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
                })?;
            v.get(0)
                .ok_or_else(|| Error::InstanceMissing(name.as_ref().to_string()))?
                .clone()
        };
        let wordpress_status = match wordpress.status {
            Some(x) if x.to_lowercase() == "created" => Status::Offline,
            Some(x) if x.to_lowercase() == "restarting" => Status::Starting,
            Some(x) if x.to_lowercase() == "running" => Status::Running,
            Some(x) if x.to_lowercase() == "removing" => Status::Offline,
            Some(x) if x.to_lowercase() == "paused" => Status::Offline,
            Some(x) if x.to_lowercase() == "exited" => Status::Offline,
            Some(x) if x.to_lowercase() == "dead" => Status::Failing,
            _ => Status::Failing,
        };
        let database_status = match database.status.clone() {
            Some(x) if x.to_lowercase() == "created" => Status::Offline,
            Some(x) if x.to_lowercase() == "restarting" => Status::Starting,
            Some(x) if x.to_lowercase() == "running" => Status::Running,
            Some(x) if x.to_lowercase() == "removing" => Status::Offline,
            Some(x) if x.to_lowercase() == "paused" => Status::Offline,
            Some(x) if x.to_lowercase() == "exited" => Status::Offline,
            Some(x) if x.to_lowercase() == "dead" => Status::Failing,
            _ => Status::Failing,
        };
        Ok(InstanceInfo {
            name: name.as_ref().to_string(),
            wordpress_status,
            database_status,
            created: wordpress
                .created
                .unwrap_or_else(|| database.created.unwrap_or_default()),
            labels: hash_map! {},
        })
    }

    async fn create<T: AsRef<str> + Send>(&self, name: T) -> Result<InstanceInfo> {
        let n = name.as_ref();
        self.docker_instance
            .create_container(
                Some(CreateContainerOptions {
                    name: &format!("{}-db", n),
                }),
                Config {
                    env: Some(vec![
                        "MYSQL_DATABASE=wordpress",
                        &format!("MYSQL_USER={}", self.config.db_username),
                        &format!("MYSQL_PASSWORD={}", self.config.db_password),
                        "MYSQL_RANDOM_ROOT_PASSWORD=1",
                    ]),
                    image: Some("mysql:5.7"),
                    labels: Some(hash_map! {
                        DB_LABEL => "",
                        "prometheus.makepress.name" => n
                    }),
                    host_config: Some(HostConfig {
                        network_mode: Some(self.config.network_name.clone()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
        self.docker_instance
            .create_container(
                Some(CreateContainerOptions { name: n }),
                Config {
                    env: Some(vec![
                        &format!("WORDPRESS_DB_HOST={}-db", n) as &str,
                        &format!("WORDPRESS_DB_USER={}", self.config.db_username),
                        &format!("WORDPRESS_DB_PASSWORD={}", self.config.db_password),
                        "WORDPRESS_DB_NAME=wordpress",
                        &format!("VIRTUAL_HOST={}.{}", n, self.config.domain),
                    ]),
                    image: Some("wordpress"),
                    labels: Some(hash_map! {
                        CONTAINER_LABEL => "",
                        "prometheus.makepress.name" => n
                    }),
                    host_config: Some(HostConfig {
                        network_mode: Some(self.config.network_name.clone()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
        self.get(name).await
    }

    async fn start<T: AsRef<str> + Send>(&self, name: T) -> Result<InstanceInfo> {
        let n = name.as_ref();
        self.docker_instance
            .start_container::<&str>(&format!("{}-db", n), None)
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
        self.docker_instance
            .start_container::<&str>(n, None)
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
        self.get(name).await
    }

    async fn stop<T: AsRef<str> + Send>(&self, name: T) -> Result<InstanceInfo> {
        let n = name.as_ref();
        self.docker_instance
            .stop_container(n, None)
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
        self.docker_instance
            .stop_container(&format!("{}-db", n), None)
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
        self.get(name).await
    }

    async fn destroy<T: AsRef<str> + Send>(&self, name: T) -> Result<()> {
        let n = name.as_ref();
        self.docker_instance
            .remove_container(
                n,
                Some(RemoveContainerOptions {
                    v: true,
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
        self.docker_instance
            .remove_container(
                &format!("{}-db", n),
                Some(RemoveContainerOptions {
                    v: true,
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })
    }
}

impl ContainerManager {
    pub fn new(docker_instance: Docker, config: Conf) -> Self {
        Self {
            docker_instance,
            config,
        }
    }

    pub async fn create_from_envs(docker_instance: Docker) -> Result<Self> {
        let config = Conf::from_envs();
        Self::create(docker_instance, config).await
    }

    pub async fn create(docker_instance: Docker, config: Conf) -> Result<Self> {
        let s = Self::new(docker_instance, config);

        s.init().await?;

        Ok(s)
    }

    pub async fn init(&self) -> Result<()> {
        flushed_print!("Checking for network...");
        if !self.check_network().await? {
            flushed_print!("MISSING\nCreating network...");
            self.create_network().await?;
            println!("DONE");
        }

        flushed_print!("FOUND\nChecking for proxy...");
        match self.get_proxy().await? {
            Some(proxy) if proxy.state != Some("running".to_string()) => {
                println!("NOT RUNNING");
                flushed_print!("Starting proxy...");
                self.docker_instance
                    .start_container::<&str>(&proxy.names.as_ref().unwrap()[0], None)
                    .await
                    .map_err::<Error, _>(|e| {
                        (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
                    })?;
                println!("DONE;")
            }
            None => {
                println!("MISSING");
                flushed_print!("Creating proxy...");
                self.create_proxy().await?;
                flushed_print!("DONE\nStarting proxy...");
                self.docker_instance
                    .start_container::<&str>("makepress-proxy", None)
                    .await
                    .map_err::<Error, _>(|e| {
                        (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
                    })?;
                println!("DONE");
            }
            _ => println!("FOUND"),
        }
        println!("READY! 🚀");
        Ok(())
    }

    async fn check_network(&self) -> Result<bool> {
        self.docker_instance
            .list_networks(Some(ListNetworksOptions {
                filters: hash_map! {
                    "label" => vec![&self.config.network_name as &str],
                    "name" => vec![&self.config.network_name as &str],
                },
            }))
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })
            .map(|v| !v.is_empty())
    }

    async fn create_network(&self) -> Result<()> {
        self.docker_instance
            .create_network(CreateNetworkOptions {
                name: &self.config.network_name as &str,
                check_duplicate: true,
                driver: "bridge",
                internal: false,
                attachable: false,
                ingress: false,
                ipam: Ipam {
                    ..Default::default()
                },
                enable_ipv6: false,
                options: hash_map! {},
                labels: hash_map! {
                    &self.config.network_name as &str => ""
                },
            })
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
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
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?
            .get(0)
            .cloned())
    }

    async fn create_proxy(&self) -> Result<()> {
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
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
        Ok(())
    }
}
