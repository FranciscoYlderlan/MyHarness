// Entrada do `pnpm dev:simulado`: o Vite põe este script antes do `src/main.tsx` (ver
// `vite.config.ts`), então os comandos da tela já encontram o núcleo simulado.
// `?cenario=<nome>` escolhe o estado em que a TEL-02 abre (lista em `./cenarios`).
// `?atraso=<ms>` muda o tempo até o login simulado concluir.
import { cenarioDaUrl } from "./cenarios";
import { instalarNucleoSimulado } from "./nucleo-simulado";

const busca = window.location.search;
const atraso = Number(new URLSearchParams(busca).get("atraso"));
instalarNucleoSimulado({
  cenario: cenarioDaUrl(busca),
  ...(Number.isFinite(atraso) && atraso > 0 ? { atrasoMs: atraso } : {}),
});
