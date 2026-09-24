# Contributing

Thanks for your interest in the project.

## Current status: issues only

The project **does not accept pull requests yet**. This will change later, and this file will
be updated when it does. Until then:

- Pull requests opened by people other than the maintainer may be closed without review.
  This is not a judgment on the change itself.
- **Issues are welcome**: bug reports, feature requests, questions about behavior, and
  documentation problems.
- If you have a fix in mind, describe it in an issue. A clear description, a reproduction, or a
  small snippet in the issue is often as useful as a patch.

Automated dependency updates from Dependabot are the only exception.

## Before opening an issue

1. Search the existing issues, open and closed. Add a comment or a reaction instead of
   opening a duplicate.
2. Check that you are on the latest release or the latest commit of the default branch.
3. Use the issue form that fits: bug report or feature request. Questions and usage help
   belong in the channels described in [SUPPORT.md](SUPPORT.md).
4. Do not report security problems in a public issue. Follow [SECURITY.md](SECURITY.md).

## Writing a good bug report

- What you did, what you expected, and what happened instead.
- The smallest steps or input that reproduce it.
- Version or commit, operating system, and Rust toolchain version (`rustc --version`).
- Logs or error output, as text and not as a screenshot.
- Remove secrets first: tokens, API keys, account identifiers, and anything else private.

## Writing a good feature request

Start from the problem you are trying to solve, then the change you propose. Requests that
describe a real use case are easier to weigh than requests that only name a solution. Small,
focused requests are easier to act on than large ones.

## Use of AI tools

AI assistants are allowed, for code, for text, and for issue reports. They must be disclosed,
so that the work gets the extra check it needs.

- Say so in the issue (the forms have a field for it) and, once pull requests are open, in
  the pull request: which tool, and what it was used for.
- You are responsible for everything you submit. Read it, run it, and understand it. Do not
  submit output you have not verified.
- Do not paste unreviewed model output as a bug report. Reports must describe something you
  actually observed.
- Undisclosed AI-generated contributions may be closed. Disclosed ones are welcome and get
  the same review as any other, with extra attention to correctness and licensing.

## Licensing

The project is licensed under the [Apache License 2.0](LICENSE). Unless you state
otherwise, anything you intentionally submit for inclusion is licensed under the same terms
(section 5 of that license). Only submit work you have the right to license this way.

## Conduct

Everyone taking part is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## When pull requests open

This section will be filled in with the workflow, code style, checks that must pass, and how
review works. Expect the usual: format with `cargo fmt`, no clippy warnings, tests passing,
and one focused change per pull request.
