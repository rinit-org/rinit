# rinit

**rinit** is a next generation init and service manager for Linux and written in Rust.
It is inspired by [66](https://web.obarun.org/software/66),
[s6](https://skarnet.org/software/s6/) suite and [deamontools](http://cr.yp.to/daemontools.html).

NOTE: This is still a work in progress, please run at your own risk.

This project was initially written in C++ and called [tt](https://github.com/danyspin97/tt).

## Features

- Support for different types of programs: _oneshot_ and _deamons_
- Predictable dependencies at build time
- Configurable parameters for services
- Asynchronous start of the services
- Log everything into plain-text files
- Low footprint
- Target desktop and servers
- Conditional dependencies for supporting various scenarios
- Provide sane defaults
- Provide user services to other init/service managers

## Getting started

### Build

_rinit_ only requires the rust compiler to build, it has no dependency.

To build the project:

```bash
$ cargo build --release
```

### Install

_rinit_ is composed by 2 binaries: `rctl` and `rinstall`.

#### rinstall

Follow usage instructions in rinstall documentation
[here](https://github.com/danyspin97/rinstall#usage).

### Services

_rinit_ requires services files to know what should run and how. These services can be provided
by your distribution, a project developer or a third party. _rinit_ provides a set of
services available and always up-to-date [here](https://github.com/rinit-org/rinit).

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

All three of them requires to have `rctl` and `rsvc` into your `PATH`.

Currently the only mode tested is the _user mode_.

### User mode

After having one or more services enabled in _rinit_, run `rsvc` as your current user.

## License

rinit is licensed under the [GPL-3.0+](/LICENSE.md) license.
