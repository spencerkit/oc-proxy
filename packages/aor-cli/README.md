# AI Open Router CLI

Install globally:

```bash
npm i -g @spencer-kit/aor
```

Start service in background:

```bash
aor start --port 8899
```

Stop service:

```bash
aor stop
```

Restart service:

```bash
aor restart --port 8899
```

Show status:

```bash
aor status
```

Set a remote management password:

```bash
aor admin-password set your-secure-password
```

Recommended for shell safety:

```bash
printf '%s\n' 'your-secure-password' | aor admin-password set --password-stdin
```

Clear the remote management password:

```bash
aor admin-password clear
```

Management UI:

```
http://127.0.0.1:8899/management
```

Environment:

- `AOR_APP_DATA_DIR` overrides the config/data directory used by `aor`.
- `AOR_CLI_HOME` overrides CLI runtime directory (default `~/.aor`).
