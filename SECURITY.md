# Security Policy

## Supported Versions

octopeek is pre-1.0 and under active development. Only the latest released version is supported for security fixes.

| Version  | Supported |
| -------- | --------- |
| 0.1.x    | Yes       |
| < 0.1    | No        |

## Reporting a Vulnerability

Please do **not** open a public GitHub issue for security vulnerabilities.

Instead, report privately to **luiseduardo.boiko@gmail.com** with:

- A description of the issue and its impact.
- Steps to reproduce (minimal test case preferred).
- The version of octopeek and your environment (OS, terminal, Rust toolchain).

You can expect an initial acknowledgement within **5 business days**. We aim to provide a fix or mitigation plan within **30 days** of a confirmed report, and will coordinate disclosure with you.

## Scope

In scope:
- The octopeek binary and its source code in this repository.
- Default configuration files and documented configuration paths.
- Handling of GitHub tokens and secrets.

Out of scope:
- Vulnerabilities in upstream dependencies (please report those to the respective projects; we'll track and update).
- Issues that require a compromised local machine or GitHub account.
