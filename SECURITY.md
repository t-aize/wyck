# Security Policy

## Supported versions

Security fixes are made on the latest release and on the default branch. Older versions do
not receive fixes.

| Version                   | Supported |
| ------------------------- | --------- |
| Latest release            | Yes       |
| Default branch            | Yes       |
| Anything older            | No        |

## Reporting a vulnerability

**Do not open a public issue, discussion, or pull request for a security problem.**

Use GitHub private vulnerability reporting:

1. Open the **Security** tab of this repository.
2. Choose **Report a vulnerability**.
3. Fill in the form.

If that option is not available, open an issue that says only that you need a private way to
report a security problem, with no technical details, and a maintainer will arrange one.

## What to include

- The affected version or commit.
- A description of the problem and its impact.
- Steps to reproduce, or a proof of concept.
- Any suggested fix or mitigation, if you have one.
- Whether you used AI tools to find or write up the report. This is allowed, but the report
  must describe something you have confirmed yourself.

Never include real credentials, tokens, or private data in a report.

## What to expect

- An acknowledgement within 5 days.
- An assessment and a rough plan within 14 days.
- Updates as work progresses, and notice before the fix is published.
- Credit in the advisory if you want it.

These are goals for a small project, not guarantees. If you hear nothing in that time, follow
up on the same private report.

## Disclosure

Please give the maintainers reasonable time to fix the problem before sharing details
publicly. The default is coordinated disclosure: the fix is released first, then an advisory
is published. Reporters who act in good faith and follow this policy will not face legal
action from the maintainers.

## Scope

In scope: vulnerabilities in the code and configuration of this repository, including how it
stores or handles credentials and secrets.

Out of scope:

- Vulnerabilities in third-party dependencies with no effect on this project. Report those
  upstream. Reports about a dependency that does affect this project are welcome.
- Problems that need physical access to an unlocked machine, or an already compromised
  operating system.
- Social engineering, and denial of service through sheer volume of traffic.
- Findings from automated scanners with no demonstrated impact.

## Dependencies

Dependencies are monitored with Dependabot and audited against the RustSec advisory
database.
