# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in StorByte, please report it **privately** rather than opening a public GitHub issue.

- **Contact:** Send vulnerability details to `[CONTACT_EMAIL]` (including steps to reproduce, affected components, and potential impact).
- Please do not publicly disclose leaked secrets, OAuth credentials, or exploit details until the issue has been addressed.

## Handling Secrets & Credentials

- **Never commit sensitive files:** Do not commit `.env` files, Google OAuth client secrets, or refresh tokens to the repository (see the **Security & privacy** section in `README.md`).
- **Desktop OAuth Tradeoff:** Because StorByte is a desktop application, OAuth client secrets bundled into desktop builds represent a known desktop-OAuth architectural tradeoff and are not considered a vulnerability on their own. However, please report any unexpected token exposure, insecure local credential storage, or unauthorized token handling bugs.