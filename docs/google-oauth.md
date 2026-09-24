# Google OAuth setup (required)

Stroex talks to Google Drive **directly from your PC**. You need your own Google Cloud OAuth client.

## 1. Create a Google Cloud project

1. Open [Google Cloud Console](https://console.cloud.google.com/)
2. Create a project (e.g. `Stroex`)
3. **APIs & Services → Library** → enable **Google Drive API**

## 2. Configure the OAuth consent screen

1. **APIs & Services → OAuth consent screen**
2. User type: **External** (or Internal if Workspace-only)
3. App name: `Stroex`
4. Add your email as developer / test user
5. Scopes (add while editing):
  - `https://www.googleapis.com/auth/drive`
  - `https://www.googleapis.com/auth/userinfo.email`
  - `openid`
6. While developing, keep the app in **Testing** and add your Gmail as a **Test user**

## 3. Create a Desktop OAuth client

1. **APIs & Services → Credentials → Create credentials → OAuth client ID**
2. Application type: **Desktop app**
3. Name: `Stroex Desktop`
4. Copy **Client ID** and **Client secret**

## 4. Redirect URI

Stroex uses a local loopback listener:

```text
http://127.0.0.1:17832/oauth/callback
```

For Desktop clients Google usually accepts loopback automatically. If your console asks for redirect URIs, add that exact URL.

# Copy from example then edit:

copy .env.example .env

# See docs/google-oauth.md for full Google Cloud Console steps.

## 6. Connect in Stroex

1. Run the app (`npm run tauri dev`)
2. Click **Connect Google Drive**
3. Browser opens → sign in → Allow
4. Return to Stroex → **Sync metadata** → **Mount Drive**

## Security notes

- Stroex never asks for your Google password
- Refresh tokens are stored in **Windows Credential Manager**, not plain text in SQLite
- File bytes go **PC ↔ Google** only — not through the Stroex API server

## Multiple accounts

Use **+ Add Google Account** for each mailbox. All accounts share one `V:\` namespace. New files go to the **default** account (first connected unless you change it later).