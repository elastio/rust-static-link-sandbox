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
    ContainerOptions, Docker,
};
use std::future::Future;

static ENVIRONMENTS: Lazy<Vec<Environment>> = Lazy::new(|| {
    vec![
        Environment::new("alpine-custom-rust", "x86_64-linux-alpine-musl"),
        Environment::new("alpine-official-rust", "x86_64-linux-unknown-musl"),
        Environment::new("debian-rust", "x86_64-linux-unknown-musl"),
    ]
});
static ENVIRONMENT_NAMES: Lazy<Vec<&'static str>> =
    Lazy::new(|| ENVIRONMENTS.iter().map(|env| env.name.as_str()).collect());

/// Describes a build environment, encapsulated in a Docker container, in which we will attempt to build a static Rust binary
pub(crate) struct Environment {
    /// The name of this environment for reporting purposes, and also the Docker image label
    /// which locates the Docker image for this environment
    pub name: String,

    /// The name of the Rust musl target.
    ///
    /// This is typicaly `x86_64-linux-unknown-musl` but Alpine comes with a custom build of Rust that uses a different name
    pub musl_target: String,
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

    /// Launch a new docker container with this environment's image
    pub async fn run_in_docker<'docker, E, S, Vols, Vol>(
        &self,
        docker: &'docker Docker,
        envs: E,
        volumes: Vols,
    ) -> Result<ContainerCreateInfo>
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
            .attach_stderr(true)
            .attach_stdout(true)
            .attach_stdin(false)
            .tty(false)
            .env(envs)
            .volumes(vols)
            .build();
        Ok(docker.containers().create(&options).await?)
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
