// Entrada do `pnpm dev:simulado`: o Vite põe este script antes do `src/main.tsx` (ver
// `vite.config.ts`), então os comandos da tela já encontram o núcleo simulado.
// `?atraso=<ms>` na URL muda o tempo até o login simulado concluir.
import { instalarNucleoSimulado } from "./nucleo-simulado";

const atraso = Number(new URLSearchParams(window.location.search).get("atraso"));
instalarNucleoSimulado(Number.isFinite(atraso) && atraso > 0 ? { atrasoMs: atraso } : {});
