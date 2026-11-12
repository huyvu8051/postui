# Security policy

## Supported versions

postui is pre-1.0. Security fixes go into the latest commit on `master`;
there are no maintained release branches yet.

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub:
**Security → Report a vulnerability** on
[github.com/huyvu8051/postui](https://github.com/huyvu8051/postui/security/advisories/new).
Don't open a public issue.

Include what you found, how to reproduce it, and the impact you expect. You
can expect an acknowledgement within a week; once a fix is ready we'll agree
on a disclosure date and credit you unless you prefer otherwise.

## Scope

In scope, for example:

- Escaping the Lua script sandbox (file system, process or network access
  from a pre-request or post-response script).
- A crafted response, collection or data file that makes postui execute code,
  write outside its data directory, or leak data to a third party.

Known behaviour, not vulnerabilities:

- postui stores workspaces, environment values (including tokens), drafts and
  response bodies as plain JSON in its data directory (see
  [docs/data-and-storage.md](docs/data-and-storage.md)). Protect that
  directory with your operating system's file permissions.
- Requests go wherever their URL points, including private network addresses;
  that's the purpose of an HTTP client.
- `F4` in the Response pane opens a link from the response in your browser,
  only when you press it.
