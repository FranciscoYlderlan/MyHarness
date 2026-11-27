// Links externos da TEL-02: na janela do Tauri, o clique chama o `openUrl` do plugin
// `opener` com a URL certa e não navega a janela; no navegador, o `<a>` abre sozinho.
import { act, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { openUrl } from "@tauri-apps/plugin-opener";
import { MainWindow } from "@/app/main-window";
import { mockCommands } from "@/bindings";
import {
  criarNucleoSimuladoDeIntegracoes,
  erroDoCatalogo,
  type NucleoSimuladoDeIntegracoes,
} from "@/lib/ipc/nucleo-simulado-integracoes";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(() => Promise.resolve()) }));

const naJanelaDoTauri = (sim: boolean) => {
  (globalThis as { isTauri?: boolean }).isTauri = sim;
};

afterEach(() => {
  clearMocks();
  vi.mocked(openUrl).mockClear();
  delete (globalThis as { isTauri?: boolean }).isTauri;
});

const todasConectadas = {
  claude_cli: { status: "conectado", versao: "2.1.0 (Claude Code)" },
  obsidian_cli: { status: "conectado" },
  github: { status: "conectado", conta: "FranciscoYlderlan" },
} as const;

function iniciar(
  inicial: Parameters<typeof criarNucleoSimuladoDeIntegracoes>[0],
): NucleoSimuladoDeIntegracoes {
  const nucleo = criarNucleoSimuladoDeIntegracoes(inicial);
  mockCommands(nucleo.handlers);
  render(<MainWindow />);
  return nucleo;
}

const indicador = () => screen.getByRole("button", { name: /^Integrações:/, hidden: true });
const cartao = (nome: string) => screen.getByRole("article", { name: nome });

async function abrirPainel() {
  await waitFor(() => expect(indicador()).not.toHaveAttribute("data-situacao", "carregando"));
  act(() => indicador().click());
  const painel = await screen.findByRole("dialog", { name: "Integrações" });
  await waitFor(() => expect(within(painel).queryAllByText("checando")).toHaveLength(0));
  return painel;
}

/** Clica no link e devolve se a navegação do `<a>` foi cancelada. */
function clicar(link: HTMLElement): boolean {
  const evento = new MouseEvent("click", { bubbles: true, cancelable: true });
  act(() => {
    link.dispatchEvent(evento);
  });
  return evento.defaultPrevented;
}

async function linkDoDevice(): Promise<HTMLElement> {
  const nucleo = iniciar({ ...todasConectadas, github: { status: "desconectado" } });
  nucleo.roteiroDeConexao("github", {
    instrucoes: [
      {
        tipo: "codigo_do_dispositivo",
        codigo: "WDJB-MJHT",
        endereco: "https://github.com/login/device",
      },
    ],
    aguardarHumano: true,
  });
  await abrirPainel();
  const c = cartao("GitHub");
  act(() => within(c).getByRole("button", { name: "Conectar" }).click());
  return within(c).findByRole("link", { name: "github.com/login/device" });
}

async function linkDeInstalacao(): Promise<HTMLElement> {
  iniciar({ ...todasConectadas, claude_cli: { status: "nao_instalado" } });
  await abrirPainel();
  return within(cartao("Claude CLI")).getByRole("link", { name: "Instalar o Claude CLI" });
}

async function linkAbrirObsidian(): Promise<HTMLElement> {
  const nucleo = iniciar(todasConectadas);
  await waitFor(() => expect(indicador()).toHaveAttribute("data-situacao", "conectado"));
  nucleo.falharChecagem(
    "obsidian_cli",
    erroDoCatalogo("OBSIDIAN_FECHADO", "ERR-03", {
      integracao: "Obsidian CLI",
      mensagem_da_cli: "The CLI is unable to find Obsidian. Make sure Obsidian is running.",
    }),
  );
  await abrirPainel();
  return within(cartao("Obsidian CLI")).getByRole("link", { name: "Abrir Obsidian" });
}

describe("links externos da TEL-02 na janela do Tauri", () => {
  beforeEach(() => naJanelaDoTauri(true));

  it("AC-01: o link do device abre pelo sistema, com a URL do GitHub", async () => {
    const link = await linkDoDevice();
    expect(clicar(link)).toBe(true);
    expect(openUrl).toHaveBeenCalledExactlyOnceWith("https://github.com/login/device");
  });

  it("AC-03: o link de instalação do Claude CLI abre pelo sistema (ERR-01)", async () => {
    const link = await linkDeInstalacao();
    expect(clicar(link)).toBe(true);
    expect(openUrl).toHaveBeenCalledExactlyOnceWith(
      "https://docs.claude.com/en/docs/claude-code/setup",
    );
  });

  it("AC-05: Abrir Obsidian abre pelo sistema, com obsidian://open (ERR-03)", async () => {
    const link = await linkAbrirObsidian();
    expect(clicar(link)).toBe(true);
    expect(openUrl).toHaveBeenCalledExactlyOnceWith("obsidian://open");
  });
});

describe("links externos da TEL-02 no navegador (dev:simulado)", () => {
  beforeEach(() => naJanelaDoTauri(false));

  it("o link do device segue como link comum, em aba nova", async () => {
    const link = await linkDoDevice();
    expect(link).toHaveAttribute("target", "_blank");
    expect(clicar(link)).toBe(false);
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("o link de instalação segue como link comum, em aba nova", async () => {
    const link = await linkDeInstalacao();
    expect(link).toHaveAttribute("target", "_blank");
    expect(clicar(link)).toBe(false);
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("Abrir Obsidian segue como link comum do protocolo, na mesma aba", async () => {
    const link = await linkAbrirObsidian();
    expect(link).toHaveAttribute("href", "obsidian://open");
    expect(link).not.toHaveAttribute("target");
    expect(clicar(link)).toBe(false);
    expect(openUrl).not.toHaveBeenCalled();
  });
});
