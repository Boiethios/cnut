# Casper Network Utility Testing

## Must be installed

- `rustup`
- `cargo`

## General specification

### Goals

#### 1. Providing binaries

The binaries are provided by:
- Compiling the local code,
- Compiling an older version given a git tag or a commit hash,
- (Optional) Downloading them from the internet (AWS artifacts).

#### 2. Generating the assets

The tool handles the needed assets:
- Chainspec;
- Node config;
- Private keys for the nodes;
- Initial balances.

This must be transparent for the user if possible. Some sane default are provided if the user does not want to bother, with of course the ability to change them if a further configuration is needed.

#### 3. Monitoring

An API is provided to access the various information we want to monitor:
- Logs;
- Memory usage;
- CPU load? (not sure if that is needed or even possible)

This information is available through a stream and/or by getting the full data at once, for example to crate a graph, or to export the full logs in a file.

#### 4. Runtime operations

All of the usual runtime operations are available:
- Upgrading nodes;
- Adding deploys;
- Starting new nodes for joining the network, retarting nodes, stopping nodes;
- Accessing internal data if needed.

### Non-goals

#### 1. Checks

The various checks are to be provided by another tool, most likely casper-test. What are provided are APIs that this other tool can use to assert.

#### 2. Scripting

Same as above. This may be provided by another tool, using the API. Whether it is desirable or not to merge it with casper-test (for example) seems out of scope.

## User-facing requirements: convenience

The tool must be easy to use for the most common scenarii: the user should not have to bother with setting things up manually. Examples include (non-exhaustive):
- Running a network using the local code is a single command.
- Running an older version of the network and upgrading the nodes one by one must not be a hassle.

## Technical

Generally speaking, this tool is a library. To use this library, the following utilities are provided:
- A web API to present the operations to the outside from the node: running the network, restarting a node, upgrading a node, streaming the logs, etc.
- A CLI to run these operations;
- An UI to display this information in a nice way. It would be good to have a view to the logs, graphs of the used memory, buttons to restart/stop nodes, etc. especially for non-technical users.
