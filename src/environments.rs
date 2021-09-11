use color_eyre::{
    eyre::Context,
    eyre::{eyre, WrapErr},
    Report, Result,
};
use once_cell::sync::Lazy;
use serde::Serialize;
use shiplift::{
    builder::{ImageFilter, ImageListOptions},
    rep::{ContainerCreateInfo, Image},
    Container, ContainerOptions, Docker,
};
use std::{
    future::Future,
    path::{Path, PathBuf},
};
use tracing::*;

static ENVIRONMENTS: Lazy<Vec<Environment>> = Lazy::new(|| {
    vec![
        Environment::new("alpine-custom-rust", "x86_64-alpine-linux-musl"),
        Environment::new("alpine-official-rust", "x86_64-unknown-linux-musl"),
        Environment::new("debian-rust", "x86_64-unknown-linux-musl"),
    ]
});
static ENVIRONMENT_NAMES: Lazy<Vec<&'static str>> =
    Lazy::new(|| ENVIRONMENTS.iter().map(|env| env.name.as_str()).collect());

/// Describes a build environment, encapsulated in a Docker container, in which we will attempt to build a static Rust binary
pub(crate) struct Environment {
    /// The name of this environment for reporting purposes, and also the Docker image label
    /// which locates the Docker image for this environment
    name: String,

    /// The name of the Rust musl target.
    ///
    /// This is typicaly `x86_64-linux-unknown-musl` but Alpine comes with a custom build of Rust that uses a different name
    musl_target: String,
}

impl Environment {
    fn new(name: &str, musl_target: &str) -> Self {
        Self {
            name: name.to_string(),
            musl_target: musl_target.to_string(),
        }
    }

    pub fn from_name(name: &str) -> Option<&Environment> {
        ENVIRONMENTS.iter().find(|env| env.name == name)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn musl_target(&self) -> &str {
        &self.musl_target
    }

    pub fn cargo_home(&self) -> &'static str {
        "/root/.cargo"
    }

    /// Launch a new docker container with this environment's image, with the working directory
    /// pre-set to `/build`
    pub async fn launch_container<'docker, E, S, Vols, Vol>(
        &self,
        docker: &'docker Docker,
        envs: E,
        volumes: Vols,
    ) -> Result<Container<'docker>>
    where
        S: AsRef<str> + Serialize,
        E: AsRef<[S]> + Serialize,
        Vol: AsRef<str>,
        Vols: AsRef<[Vol]>,
    {
        let image = self.find_docker_image(docker).await?;

        // Create a new container running this image
        let vols = volumes.as_ref().iter().map(|v| v.as_ref()).collect();
        let options = ContainerOptions::builder(&image.id)
            .tty(true)
            .env(envs)
            .volumes(vols)
            .auto_remove(false)
            .working_dir("/build")
            // .user("1000")
            .build();
        let container_info = docker
            .containers()
            .create(&options)
            .await
            .wrap_err_with(|| eyre!("Error creating docker container from image {}", image.id))?;

        debug!(container_id = %container_info.id, "Started container");

        let container: Container<'docker> = docker.containers().get(&container_info.id);

        container
            .start()
            .await
            .wrap_err_with(|| eyre!("Error starting docker container from image {}", image.id))?;

        Ok(container)
    }

    /// Find the docker image for this environment in the local docker daemon
    async fn find_docker_image(&self, docker: &shiplift::Docker) -> Result<shiplift::rep::Image> {
        crate::docker::get_image_by_label(docker, &self.name).await
    }
}

pub(crate) fn all_environments() -> &'static [Environment] {
    ENVIRONMENTS.as_slice()
}

pub(crate) fn all_environment_names() -> &'static [&'static str] {
    ENVIRONMENT_NAMES.as_slice()
}
