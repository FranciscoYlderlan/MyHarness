# MyHarness

Aplicação desktop (Tauri 2 + Rust + React) que governa o trabalho de agentes do Claude CLI, usa o cofre Obsidian só como memória e deixa o humano como validador final.

> Estado: esqueleto inicial. Só o ambiente de desenvolvimento existe; o código do app nasce na etapa 2 do roteiro (fundação), depois do aval da arquitetura e da stack.

## Ambiente de desenvolvimento (WSL + Docker)

Pré-requisitos: Windows 11 com WSL 2 (WSLg ativo) e Docker disponível dentro do WSL (Docker Desktop com integração WSL ou Docker Engine no próprio WSL).

```bash
# dentro do WSL, na pasta do repositório
cp .env.example .env        # ajuste USER_UID/USER_GID com `id -u` e `id -g`
docker compose build
docker compose up -d dev
docker compose exec dev bash
```

| Comando | Para quê |
|---|---|
| `docker compose up -d dev` | Container de trabalho, com Rust, Node, pnpm e Claude CLI |
| `docker compose exec dev claude` | Primeiro login do Claude CLI deste ambiente (fica no volume `claude-home`) |
| `docker compose run --rm check` | Checagem completa (`pnpm check`: lint, tipos, testes) |
| `docker compose --profile gui up app` | Abre a janela do app pelo WSLg |
| `http://localhost:1420` | Tela no navegador, com o núcleo simulado |

Dica de desempenho: guarde o repositório dentro do sistema de arquivos do WSL (`~/projetos/my-harness`) ou, se ficar em `C:\Users\...\Projetos\my-harness`, espere builds mais lentos pelo acesso a `/mnt/c`.

## Comandos

Depois de `pnpm install` (dentro do container ou numa máquina com Rust, Node 22 e pnpm):

| Comando | Para quê |
|---|---|
| `pnpm dev` | Só a tela (Vite em `http://localhost:1420`), sem o núcleo Rust |
| `pnpm tauri dev` | App completo: janela do Tauri com recarga da tela e do núcleo |
| `pnpm check` | Checagem completa: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `tsc --noEmit`, ESLint, Prettier `--check` e Vitest |
| `pnpm tauri build --debug --no-bundle` | Binário de depuração do app, sem instalador |

O `pnpm check` também roda no `pre-push` (lefthook) e no CI (`.github/workflows/ci.yml`, Windows e Ubuntu).

## Limites conhecidos

- **Obsidian:** roda no Windows e é acessado **via MCP**. No Windows fica um servidor MCP da Obsidian CLI com transporte HTTP; o container o alcança em `OBSIDIAN_MCP_URL` (padrão `http://host.docker.internal:3901/mcp`, ver `.env.example`). O Obsidian precisa estar aberto. O servidor HTTP nasce na etapa 2.
- **Plataforma final:** o app roda no Windows. O container serve para desenvolver e testar; o instalador final é gerado no Windows.
- **Cofre de credenciais:** no app final, o Gerenciador de Credenciais do Windows; no container, o `keyring` usa o Secret Service (gnome-keyring, sessão D-Bus).
- **Janela pelo WSLg:** depende do WSLg do Windows 11; o volume `/mnt/wslg` precisa existir no WSL.
- **Banco local:** um arquivo SQLite (`my-harness.sqlite3`, modo WAL) no diretório de dados do app (`app_data_dir` do Tauri: no Linux, `~/.local/share/dev.myharness.app/`; no Windows, `%APPDATA%\dev.myharness.app\`). As migrações de `src-tauri/migrations/` rodam sozinhas ao abrir o app.
- **Adaptadores ainda vazios:** em `src-tauri/src/infra/`, só `db` funciona; `claude`, `obsidian`, `github`, `git`, `secrets`, `process` e `mcp` são módulos vazios. `tools/fake-claude/` é só um marcador.
- **shadcn/ui ainda não instalado:** a stack prevê shadcn/ui instalado só pela CLI dele (`pnpm dlx shadcn@latest init`); falta rodar essa etapa num ambiente que alcance `ui.shadcn.com`.
