# Rust Static Link Sandbox

> In which I discover every possible way to fail to produce a static Rust binary.

## The tl;dr

It is possible to produce Rust static binaries for Linux.  It will be very easy if your project meets these criteria:

* 100% pure Rust
* Binary and all dependencies do not use `bindgen`
* Binary and all dependencies do not link with any C or C++ libraries
* Binary and all dependencies do not use the `openssl` crate
* The `cpp` crate is not used at all

It will be somewhat more complex but still possible if:
* Dependency graph includes `openssl` but no other C/C++ libraries

Unfortunately it's quite unlikely that your binary crates does anything substantial and avoids an `openssl` dependency, so odds are you're in for a slog.  Hopefully this sandbox helps you reason about your needs and how to achieve them.

## The (Seemingly) Simple Task

This exploration started with a need we had at [Elastio](https://elastio.com) to package our Rust-based CLI into a single, static executable that will run on any modern amd64 Linux distribution, including lightweight Alpine container distros and full-size Debian and RHEL VM distros.

The few words of praise I will speak for Golang are reserved for its static binaries, which for the most part Just Work and allow Go-based products like Terraform and k8s to ship portable static binaries that can be run anywhere without any dependency hell or copy-pasted `sudo dnf/yum/apt-get/puc/phuk/whatever` nonsense.  Rust sadly doesn't have such a simple static binary story to tell, for reasons I won't get into but it must be said are quite legitimate.  Go's seeming simplicity in this area, as in most areas of Golang, are illusory and conceal a plethora of footguns, gotchas, edge-cases, drawbacks, and haughty opinionated arrogance.

## The Environments

This repo includes multiple Dockerfiles, all prefixed `Dockerfile`, which define the different build environments I experimented with trying to produce a static binary.  Build them in advance with

```shell
$ ./build-docker-images.sh
```

if you want to follow along at home.

## The Crates

In the `crates/` folder there are a number of very simple Rust crates, which mostly vary in which dependencies they have or what they do in `build.rs`.  These are used to illustrate in which cases producing a static binary is easy and in which cases it's practically impossible.

## The Attempts

I'll go through the various things I've tried, some of which worked to some extent or another.  In each case there's some limitation or gotcha that made that particular approach unsuitable for Elastio, but maybe is good enough for your use case.
