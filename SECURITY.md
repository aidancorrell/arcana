# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Arcana, please report it responsibly.

**Do not open a public issue.** Instead, email **aidan.c.correll@gmail.com** with:

- A description of the vulnerability
- Steps to reproduce
- Potential impact

You will receive a response within 72 hours. We will work with you to understand the issue and coordinate a fix before any public disclosure.

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Scope

The following are in scope for security reports:

- SQL injection in store queries
- Authentication bypass in HTTP/SSE transport
- Path traversal in document ingestion
- Credential exposure in logs or error messages
- Unauthorized access to MCP tools

Out of scope:

- Denial of service via large inputs (known limitation of SQLite-backed store)
- Issues requiring physical access to the host machine
