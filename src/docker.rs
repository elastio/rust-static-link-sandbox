use color_eyre::{
    eyre::Context,
    eyre::{eyre, WrapErr},
    Report, Result,
};
use futures::StreamExt;
use shiplift::{
    builder::{ImageFilter, ImageListOptions},
    rep::{ContainerCreateInfo, ExecDetails, Image},
    tty::TtyChunk,
    Container, ContainerOptions, Docker, Exec, ExecContainerOptions,
};
use std::io::{self, Read, Write};
use tracing::*;

pub(crate) async fn connect_docker() -> Result<Docker> {
    let docker = shiplift::Docker::new();

    // Make sure it's working
    let version = docker.version().await?;

    debug!(?version, "Connected to Docker daemon");

    Ok(docker)
}

pub(crate) async fn get_image_by_label(docker: &Docker, label: &str) -> Result<Image> {
    let options = ImageListOptions::builder().build();

    let images = docker.images().list(&options).await?;

    // Find an image that has a repo tag `elastio:$label`
    let repo_tag = format!("elastio:{}", label);

    if let Some(image) = images.into_iter().find(|image| {
        image
            .repo_tags
            .as_ref()
            .map(|tags| tags.iter().any(|tag| tag == &repo_tag))
            .unwrap_or(false)
    }) {
        debug!(label, image_id = %image.id, "Found image by label");
        Ok(image)
    } else {
        Err(eyre!("No docker image found with label '{}'; did you run the `build-docker-images.sh` script?", label))
    }
}

/// Helper to run a command in a container.
///
/// This command is always run with `bash -c` to ensure env vars are set up as expected.
///
/// The stdout/stderr for this command are echoed live to this process' stdout/stderr for now,
/// and also combined into a single string which is returned along with the results of the exec operation
pub(crate) async fn exec_in_container<'docker, C, S>(
    docker: &'docker Docker,
    container: &Container<'docker>,
    cmd: C,
) -> Result<(ExecDetails, String)>
where
    C: AsRef<[S]>,
    S: AsRef<str>,
{
    // Make the command a single argument to `bash -c`
    let args = cmd
        .as_ref()
        .iter()
        .map(|c| format!("\"{}\"", c.as_ref()))
        .collect::<Vec<_>>()
        .join(" ");

    debug!(command = %args,
        "Running command in container");

    let exec_options = ExecContainerOptions::builder()
        .cmd(vec!["bash", "-c", &args])
        .attach_stderr(true)
        .attach_stdout(true)
        .build();
    let exec = Exec::create(&docker, container.id(), &exec_options)
        .await
        .wrap_err_with(|| "Error executing command in container")?;
    exec.inspect().await?;

    // For simplicity, we'll bypass the logging framework for the test, and write the output verbatim to our
    // stdout/stderr streams
    let mut stream = exec.start();
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let mut output = Vec::new();
    while let Some(item) = stream.next().await {
        match item {
            Err(e) => {
                error!("Docker exec error: {}", e);
                return Err(e.into());
            }
            Ok(TtyChunk::StdOut(chunk)) => {
                stdout.write_all(&chunk)?;
                output.write_all(&chunk)?;
            }
            Ok(TtyChunk::StdErr(chunk)) => {
                stderr.write_all(&chunk)?;
                output.write_all(&chunk)?;
            }
            Ok(TtyChunk::StdIn(_)) => {
                unreachable!()
            }
        }
    }

    // Presumably, execution has finished
    let results = exec.inspect().await?;

    if results.running {
        return Err(eyre!(
            "Process is still running when it was expected to be finished"
        ));
    }

    Ok((results, String::from_utf8_lossy(&output).to_string()))
}
