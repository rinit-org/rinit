# rinit

![GitHub branch checks state](https://img.shields.io/github/checks-status/rinit-org/rinit/main?logo=github)
[![codecov](https://codecov.io/gh/rinit-org/rinit/branch/main/graph/badge.svg?token=owkoG8w2UG)](https://codecov.io/gh/rinit-org/rinit)
![GitHub](https://img.shields.io/github/license/rinit-org/rinit?logo=github)

**rinit** is a next generation init and service manager for Linux and written in Rust.
It is inspired by [66](https://web.obarun.org/software/66),
[s6](https://skarnet.org/software/s6/) suite and [daemontools](http://cr.yp.to/daemontools.html).

NOTE: This is still a work in progress, please run at your own risk.

This project was initially written in C++ and called [tt](https://github.com/danyspin97/tt).

## Features

- Support for different types of programs: _oneshot_ and _daemons_
- Predictable dependencies at build time
- Configurable parameters for services
- Asynchronous start of the services
- Log everything into plain-text files
- Low footprint
- Target desktop and servers
- Conditional dependencies for supporting various scenarios
- Provide sane defaults
- Provide user services to other init

## Getting started

### Build

_rinit_ only requires the nightly rust compiler to build, it has no dependency.

To build the project:

```bash
$ cargo build --release
```

### Install

_rinit_ is composed by 3 binaries: `rctl`, `rsvc` and `rsupervision`.

It can be easily installed via *rinstall*. Follow usage instructions in rinstall documentation
[here](https://github.com/danyspin97/rinstall#usage).

### Services

_rinit_ requires services files to know what should run and how. These services can be provided
by your distribution, a project developer or a third party. _rinit_ provides a set of
services available and always up-to-date [here](https://github.com/rinit-org/rinit-services).

## Usage

_rinit_ keeps a graph with all the enabled services and their dependencies. To start using rinit,
enable one or more services.

### Enable a service

To enable a service, run the following command:

```bash
$ rctl enable <service>
```

The service and all its dependencies will be enabled and will be started in the next _rinit_
startup. To start a service right after enabling it, add the `--start` option:

```bash
$ rctl enable --start <service>
```

### Disable a service

To disable a service, run the following command:

```bash
$ rctl disable <service>
```

The service will continue running even after it has been disabled. To stop a service after
disabling it, add the `--stop` option:

```bash
$ rctl disable --stop <service>
```

### Start a service

To start a service, use:

```bash
$ rctl start <service>
```

### Stop a service

To stop a service, use:

```bash
$ rctl stop <service>
```

### Get current status

To get the current status of the services handled by rinit, run:

```bash
$ rctl status
```

## Modes

_rinit_ works in three different modes:

- **root mode**, working a system service manager
- **user mode**, working a user service manager
- **project mode**, working a service manager for a specific set of services

The command line interface is the same for each mode. The only difference is whom the service
affects and the directory used for the configuration.

All three of them requires to have `rsupervision` into your `PATH`. Please note that the Rust
function that is used to spawn `rsupervision` **doesn't support tilde expanding `~/` in `PATH`**.

### User mode

After having one or more services enabled in _rinit_, run `rsvc` as your current user. rinit
in user mode can be used for starting graphical daemons (like `polybar` or `waybar`), as
well as a various things, from the complete MPD setup to periodically fetch data from a
WebDAV server.

Most daemons and services requires environmental variables set at runtime from various programs,
like the `DBUS_SESSION_BUS_ADDRESS` to the `WAYLAND_DISPLAY`. For this reason it is suggested
to start `rsvc` after the compositor/window manager has started, via `.xstartrc` or
by using the autostart feature of your Desktop/window manager.

## License

rinit is licensed under the [GPL-3.0+](/LICENSE.md) license.
