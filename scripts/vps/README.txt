Room — VPS package
==================

Layout
------
  bin/room-server   release binary
  web/              static client (wasm + HTML)
  mods/             gameplay PK3s (royale, bounty, …)
  data/             persistent (created on start; not overwritten by push)
  logs/             service logs
  run/              pid files
  .env              secrets (copy from env.example; never overwrite on push)
  room              control script  (./room start|stop|status|logs|health)
  Caddyfile         optional reverse-proxy TLS
  BUILD.txt         build stamp from package-vps.sh

Quick start on the VPS
----------------------
  1. Upload this tree (or use ./scripts/push-vps.sh from your laptop).
  2. cp env.example .env && edit YOUR_VPS_IP / secrets.
  3. ./room start
  4. Open http://YOUR_IP:8765/  (lobby) and http://YOUR_IP:8766/admin

Firewall
--------
  Open TCP 8765 (game) and 8766 (admin), or put Caddy in front and only
  expose 80/443 while binding Room to 127.0.0.1 (see env.example).

Update without wiping data
--------------------------
  On the build machine:
    ./scripts/package-vps.sh
    ./scripts/push-vps.sh user@host:~/room/

  On the VPS:
    ./room restart

  push-vps keeps remote: data/  logs/  .env  run/

Admin
-----
  Bootstrap user comes from ROOM_ADMIN_BOOTSTRAP in .env (user:pass).
  After the first login, prefer WebAuthn / password change; remove the
  bootstrap line once data/admins.json exists.

Systemd (optional)
------------------
  See deploy/room-server.service — point WorkingDirectory and ExecStart
  at this tree, or keep using ./room under a process supervisor.
