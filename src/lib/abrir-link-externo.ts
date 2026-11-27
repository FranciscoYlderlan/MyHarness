// Abre um link externo fora da tela. Na janela do Tauri, o clique num `<a>` externo não
// abre nada: o pedido vai ao plugin `opener`, que usa o app padrão do sistema (escopo em
// `src-tauri/capabilities/default.json`). No navegador (`pnpm dev:simulado`), o próprio
// `<a>` abre o link, como sempre.
import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * Na janela do Tauri, entrega `url` ao sistema e devolve `true`: quem chama cancela a
 * navegação do `<a>`. Se o sistema recusar o pedido (fora do escopo, sem app para o
 * protocolo), chama `aoFalhar`, para a tela mostrar a falha. Fora da janela, devolve
 * `false` e o `<a>` segue o comportamento comum.
 */
export function abrirLinkExterno(url: string, aoFalhar?: (erro: unknown) => void): boolean {
  if (!isTauri()) return false;
  openUrl(url).catch((erro: unknown) => {
    console.error(`não foi possível abrir o link externo ${url}:`, erro);
    aoFalhar?.(erro);
  });
  return true;
}
