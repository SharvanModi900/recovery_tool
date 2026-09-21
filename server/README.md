# RecoverKit Remote Agent Server - Render Deployment

This directory contains the deployment configuration for deploying the RecoverKit remote forensic agent receiver to Render.com.

## What This Does

The server accepts forensic data packages from the RecoverKit Android agent app via HTTP POST and stores them for later processing by the desktop app.

**Endpoints:**
- `POST /upload` - Receive forensic package from Android agent
- `GET /status` - Server status and package count
- `GET /health` - Health check for Render monitoring

## Deploy to Render

### Option 1: One-Click Deploy (Recommended)

1. Push this repository to GitHub
2. Go to [render.com](https://render.com) and sign up/login
3. Click **New +** → **Web Service**
4. Connect your GitHub repository
5. Render will auto-detect the `render.yaml` configuration
6. Click **Create Web Service**

### Option 2: Manual Deploy

1. Go to [render.com](https://render.com) → **New +** → **Web Service**
2. Connect your GitHub repo
3. Configure:
   - **Name:** `recoverkit-remote-agent`
   - **Environment:** **Docker**
   - **Dockerfile Path:** `server/Dockerfile`
   - **Build Context:** `.` (root)
   - **Plan:** Free (or paid for persistent storage)
4. Add environment variables:
   - `PORT` = `7799`
   - `STORAGE_DIR` = `/app/data`
5. Add a disk (paid plan only):
   - **Name:** `data`
   - **Mount Path:** `/app/data`
   - **Size:** 1 GB
6. Click **Create Web Service**

### Option 3: Local Testing First

Test the server locally before deploying:

```powershell
# Build the server
cd D:\recovery-tool\core
cargo build --bin remote_agent_server --release

# Run it
$env:PORT = "7799"
$env:STORAGE_DIR = "D:\recovery-tool\workspace\remote_data"
.\target\release\remote_agent_server.exe
```

Then update the Android agent app to use your server URL.

## Update Android Agent App

After deployment, Render gives you a URL like:
```
https://recoverkit-remote-agent.onrender.com
```

Update the agent app's default URL in `agent/src/com/recoverkit/agent/RemoteAgent.java`:

```java
serverInput.setText("https://recoverkit-remote-agent.onrender.com");
```

Rebuild and reinstall the APK:
```powershell
# (follow the build steps from the main README)
```

## Storage Considerations

**Free Plan:**
- Ephemeral storage (data lost on restart)
- Good for testing, not for production forensics

**Paid Plan ($7/month):**
- Persistent disk storage
- Data survives restarts
- Required for actual forensic work

**Alternative: External Storage**
For production, consider:
- AWS S3 for package storage
- PostgreSQL/MySQL for metadata
- Render's managed databases

## Monitoring

Render provides:
- Health check monitoring via `/health`
- Request logs
- CPU/memory metrics
- Deploy history

## Security Notes

⚠️ **Current Implementation:**
- Plain HTTP (no encryption)
- No authentication
- Suitable for testing only

**For Production:**
- Add HTTPS (Render provides free SSL)
- Add API key authentication
- Implement rate limiting
- Add input validation
- Consider using Render's private networking

## Troubleshooting

**Server won't start:**
- Check Render logs: **Logs** tab in dashboard
- Verify `PORT` env variable is set
- Ensure Dockerfile builds successfully

**Data not persisting:**
- Free plan has ephemeral storage
- Upgrade to paid plan and add a disk
- Check `STORAGE_DIR` matches disk mount path

**Can't connect from Android:**
- Verify server URL is correct (https://...)
- Check Render dashboard shows service is **Live**
- Test health endpoint in browser: `https://your-app.onrender.com/health`

## Cost

- **Free:** $0/month (ephemeral, spins down after 15min inactivity)
- **Starter:** $7/month (persistent disk, always on)
- **Standard:** $25/month (more resources, priority support)

For forensic work, the **Starter** plan with persistent disk is recommended.
