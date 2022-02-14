use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use bollard::{
    container::{Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions},
    exec::CreateExecOptions,
    models::{ContainerSummaryInner, HostConfig, Ipam, PortBinding},
    network::{CreateNetworkOptions, ListNetworksOptions},
    volume::{CreateVolumeOptions, ListVolumesOptions},
    Docker,
};
use log::warn;
use makepress_lib::{
    uuid::Uuid, BackupAcceptedResponse, BackupCheckResponse, Error, InstanceInfo, MakepressManager,
    Status, CreateInfo,
};

use crate::{
    backup::{BackupManager, BackupState},
    config::Config as Conf,
    const_expr_count, hash_map, CONTAINER_LABEL, DB_LABEL,
};

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

    backup_manager: Arc<BackupManager>,
}

#[async_trait]
impl MakepressManager for ContainerManager {
    async fn list(&self) -> Result<Vec<String>> {
        let label = "prometheus.makepress.name";
        let mut name_set = HashSet::new();
        let containers = self
            .docker_instance
            .list_containers(Some(ListContainersOptions::<_> {
                all: true,
                filters: hash_map! {
                    "label" => vec![label],
                },
                ..Default::default()
            }))
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
        for container in containers {
            let labels = container.labels.unwrap();
            let name = labels.get("prometheus.makepress.name").unwrap();
            name_set.insert(name.clone());
        }

        Ok(name_set.into_iter().collect())
    }

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
        let wordpress_status = match wordpress.state {
            Some(x) if x.to_lowercase() == "created" => Status::Offline,
            Some(x) if x.to_lowercase() == "restarting" => Status::Starting,
            Some(x) if x.to_lowercase() == "running" => Status::Running,
            Some(x) if x.to_lowercase() == "removing" => Status::Offline,
            Some(x) if x.to_lowercase() == "paused" => Status::Offline,
            Some(x) if x.to_lowercase() == "exited" => Status::Offline,
            Some(x) if x.to_lowercase() == "dead" => Status::Failing,
            x => {
                warn!("unknown status: {:?}", x);
                Status::Failing
            },
        };
        let database_status = match database.state.clone() {
            Some(x) if x.to_lowercase() == "created" => Status::Offline,
            Some(x) if x.to_lowercase() == "restarting" => Status::Starting,
            Some(x) if x.to_lowercase() == "running" => Status::Running,
            Some(x) if x.to_lowercase() == "removing" => Status::Offline,
            Some(x) if x.to_lowercase() == "paused" => Status::Offline,
            Some(x) if x.to_lowercase() == "exited" => Status::Offline,
            Some(x) if x.to_lowercase() == "dead" => Status::Failing,
            x => {
                warn!("unknown status: {:?}", x);
                Status::Failing
            },
        };
        let host_type = match wordpress.labels.unwrap_or_default().get("prometheus.makepress.host_type") {
            Some(x) if x == "Managed" => makepress_lib::HostType::Managed,
            Some(x) if x == "Unmanaged" => makepress_lib::HostType::Unmanaged,
            _ => unreachable!("host type is not valid")
        };
        Ok(InstanceInfo {
            name: name.as_ref().to_string(),
            wordpress_status,
            database_status,
            created: wordpress
                .created
                .unwrap_or_else(|| database.created.unwrap_or_default()),
            labels: hash_map! {},
            host_type
        })
    }

    async fn create<T: AsRef<str> + Send>(&self, name: T, options: CreateInfo) -> Result<InstanceInfo> {
        let n = name.as_ref();
        if n == "api" {
            return Err(Error::Unknown);
        }
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
                        binds: Some(vec![format!("{}:/backups", self.config.backups_volume)]),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
        let domain = match options.host_type {
            makepress_lib::HostType::Managed => format!("{}.{}", n, self.config.domain),
            makepress_lib::HostType::Unmanaged => n.to_string(),
        };
        let host_label = match options.host_type {
            makepress_lib::HostType::Managed => "Managed",
            makepress_lib::HostType::Unmanaged => "Unmanaged",
        };
        self.docker_instance
            .create_container(
                Some(CreateContainerOptions { name: n }),
                Config {
                    env: Some(vec![
                        &format!("WORDPRESS_DB_HOST={}-db", n) as &str,
                        &format!("WORDPRESS_DB_USER={}", self.config.db_username),
                        &format!("WORDPRESS_DB_PASSWORD={}", self.config.db_password),
                        "WORDPRESS_DB_NAME=wordpress",
                        &format!("VIRTUAL_HOST={}", domain),
                    ]),
                    image: Some("wordpress"),
                    labels: Some(hash_map! {
                        CONTAINER_LABEL => "",
                        "prometheus.makepress.name" => n,
                        "prometheus.makepress.host_type" => host_label
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

    async fn start_backup<T: AsRef<str> + Send>(&self, name: T) -> Result<BackupAcceptedResponse> {
        let id = Uuid::new_v4();
        let s = self.clone();
        let n = name.as_ref().to_string();
        tokio::spawn(async move {
            s.backup_manager
                .set_status(id, BackupState::Running)
                .unwrap();
            let r = s
                .docker_instance
                .create_exec(
                    &format!("{}-db", n),
                    CreateExecOptions::<String> {
                        cmd: Some(vec![
                            "mysqldump".to_string(),
                            "-u".to_string(),
                            s.config.db_username,
                            "-p".to_string(),
                            s.config.db_password,
                            "wordpress".to_string(),
                            "|".to_string(),
                            "gzip".to_string(),
                            ">".to_string(),
                            format!("/backups/{}.sql.gz", id),
                        ]),
                        ..Default::default()
                    },
                )
                .await;
            s.backup_manager
                .set_status(
                    id,
                    match r {
                        Ok(_) => BackupState::Finished,
                        Err(e) => BackupState::Error(e.to_string()),
                    },
                )
                .unwrap();
        });
        Ok(BackupAcceptedResponse { job_id: id })
    }

    async fn check_backup(&self, id: Uuid) -> Result<BackupCheckResponse> {
        self.backup_manager
            .get_status(id)
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })
            .map(|status| BackupCheckResponse {
                status: match status.clone() {
                    BackupState::NotFound => "Not Found".to_string(),
                    BackupState::Pending => "Pending".to_string(),
                    BackupState::Running => "Running".to_string(),
                    BackupState::Error(e) => format!("Error: {}", e),
                    BackupState::Finished => "Finished".to_string(),
                },
                access_url: if let BackupState::Finished = status {
                    Some(format!(
                        "http://api.{}/backups/download/{}",
                        self.config.domain, id
                    ))
                } else {
                    None
                },
            })
    }

    /// This version of makepress does not yet support cancelling backups
    async fn cancel_backup(&self, _id: Uuid) -> Result<()> {
        Err(Error::IOError(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "This version of the makepress api does not support cancelling backups",
        )))
    }
}

impl ContainerManager {
    pub fn new(docker_instance: Docker, config: Conf, backup_manager: BackupManager) -> Self {
        Self {
            docker_instance,
            config,
            backup_manager: Arc::new(backup_manager),
        }
    }

    pub async fn create_from_envs(
        docker_instance: Docker,
        backup_manager: BackupManager,
    ) -> Result<Self> {
        let config = Conf::from_envs();
        Self::create(docker_instance, config, backup_manager).await
    }

    pub async fn create(
        docker_instance: Docker,
        config: Conf,
        backup_manager: BackupManager,
    ) -> Result<Self> {
        let s = Self::new(docker_instance, config, backup_manager);

        s.init().await?;

        Ok(s)
    }

    pub async fn init(&self) -> Result<()> {
        flushed_print!("Checking for network...");
        if !self.check_network().await? {
            flushed_print!("MISSING\nCreating network...");
            self.create_network().await?;
            println!("DONE");
        } else {
            println!("FOUND");
        }
        flushed_print!("Checking for backup volume...");
        if !self.check_volume().await? {
            flushed_print!("MISSING\nCreating backup volume...");
            self.create_volume().await?;
            println!("DONE")
        } else {
            println!("FOUND");
        }

        flushed_print!("Checking for proxy...");
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

    async fn check_volume(&self) -> Result<bool> {
        self.docker_instance
            .list_volumes(Some(ListVolumesOptions {
                filters: hash_map! {
                    "name" => vec![&self.config.backups_volume as &str]
                },
            }))
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })
            .map(|v| !v.volumes.is_empty())
    }

    async fn create_volume(&self) -> Result<()> {
        self.docker_instance
            .create_volume(CreateVolumeOptions::<&str> {
                name: &self.config.backups_volume,
                ..Default::default()
            })
            .await
            .map_err::<Error, _>(|e| {
                (Box::new(e) as Box<dyn std::error::Error + Send + Sync>).into()
            })?;
        Ok(())
    }
}
