// Cada cenário do `dev:simulado` abre a TEL-02 no estado dele, sobre o mesmo núcleo
// simulado dos testes da tela.
import { act, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { MainWindow } from "@/app/main-window";
import { mockCommands } from "@/bindings";
import { CENARIO_PADRAO, CENARIOS, cenarioDaUrl } from "./cenarios";
import { criarNucleoSimulado } from "./nucleo-simulado";

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

type Cartao = "Claude CLI" | "Obsidian CLI" | "GitHub";

/** O que cada cartão mostra: o status no cabeçalho e o texto do alerta (ou nenhum). */
type Esperado = Partial<Record<Cartao, { status: string; alerta?: string; botao?: string }>>;

const PADRAO: Esperado = {
  "Claude CLI": { status: "conectado", botao: "Desconectar" },
  "Obsidian CLI": { status: "desconectado", botao: "Conectar" },
  GitHub: { status: "desconectado", botao: "Conectar" },
};

const NOME_DO_PREFIXO: Record<string, Cartao> = {
  claude: "Claude CLI",
  obsidian: "Obsidian CLI",
  github: "GitHub",
};

function esperadoDeCadaIntegracao(): Record<string, Esperado> {
  const esperado: Record<string, Esperado> = {};
  // `sem login` e `não instalado` só existem para o Claude CLI (o núcleo só manda esses).
  esperado["claude-sem-login"] = { "Claude CLI": { status: "sem login", botao: "Conectar" } };
  esperado["claude-nao-instalado"] = {
    "Claude CLI": { status: "não instalado", alerta: "Claude CLI não encontrado", botao: "Testar" },
  };
  for (const [p, nome] of Object.entries(NOME_DO_PREFIXO)) {
    esperado[`${p}-err-53`] = {
      [nome]: {
        status: "desconectado",
        alerta: `Falha inesperada com ${nome}: tempo esgotado [simulado]. Tente de novo.`,
        botao: "Repetir",
      },
    };
    esperado[`${p}-err-55`] = {
      [nome]: {
        status: "desconectado",
        alerta: `Algo deu errado no MyHarness: o núcleo não ligou ${nome} [simulado]. Tente de novo.`,
        botao: "Repetir",
      },
    };
  }
  return esperado;
}

const ESPERADO: Record<string, Esperado> = {
  padrao: {},
  "todas-conectadas": {
    "Obsidian CLI": { status: "conectado", botao: "Desconectar" },
    GitHub: { status: "conectado", botao: "Desconectar" },
  },
  "claude-err-01": {
    "Claude CLI": { status: "não instalado", alerta: "Claude CLI não encontrado", botao: "Testar" },
  },
  "github-err-02": {
    GitHub: {
      status: "desconectado",
      alerta: "Conexão com o GitHub não concluída: código expirado.",
      botao: "Conectar",
    },
  },
  "obsidian-err-03": {
    "Obsidian CLI": {
      status: "desconectado",
      alerta: "The CLI is unable to find Obsidian [simulado]Abrir Obsidian",
      botao: "Conectar",
    },
  },
  "claude-err-04": {
    "Claude CLI": {
      status: "sem login",
      alerta: "o /login foi fechado antes de concluir [simulado]",
      botao: "Repetir",
    },
  },
  ...esperadoDeCadaIntegracao(),
};

const indicador = () => screen.getByRole("button", { name: /^Integrações:/, hidden: true });

async function abrirTel02(cenario: string) {
  mockCommands(criarNucleoSimulado({ cenario }).handlers);
  render(<MainWindow />);
  await waitFor(() => expect(indicador()).not.toHaveAttribute("data-situacao", "carregando"));
  act(() => indicador().click());
  const painel = await screen.findByRole("dialog", { name: "Integrações" });
  await waitFor(() => expect(within(painel).queryAllByText(/checando/)).toHaveLength(0));
}

describe("cenários do dev:simulado", () => {
  it("todo cenário tem o estado esperado da TEL-02 neste teste", () => {
    expect(Object.keys(ESPERADO).sort()).toEqual(Object.keys(CENARIOS).sort());
  });

  it.each(Object.keys(CENARIOS))("%s abre a TEL-02 no estado dele", async (nome) => {
    await abrirTel02(nome);
    const esperado = { ...PADRAO, ...ESPERADO[nome] };
    for (const [cartao, { status, alerta, botao }] of Object.entries(esperado)) {
      const c = screen.getByRole("article", { name: cartao });
      expect(within(c).getByRole("heading", { name: cartao }).nextElementSibling).toHaveTextContent(
        status,
      );
      if (alerta) expect(within(c).getByRole("alert")).toHaveTextContent(alerta);
      else expect(within(c).queryByRole("alert")).toBeNull();
      if (botao) expect(within(c).getByRole("button", { name: botao })).toBeEnabled();
    }
  });

  it("ERR-55: Conectar falha também, sem instruções, enquanto a integração não está ligada", async () => {
    await abrirTel02("github-err-55");
    const c = screen.getByRole("article", { name: "GitHub" });
    act(() => within(c).getByRole("button", { name: "Conectar" }).click());
    // Falhou no conectar: só Repetir (que repete o conectar), sem o código do device flow.
    await waitFor(() =>
      expect(within(c).queryByRole("button", { name: "Conectar" })).not.toBeInTheDocument(),
    );
    expect(within(c).getByRole("alert")).toHaveTextContent("o núcleo não ligou GitHub");
    expect(within(c).getByRole("button", { name: "Repetir" })).toBeEnabled();
    expect(within(c).queryByRole("list")).toBeNull();
  });
});

describe("?cenario= na URL", () => {
  it("sem o parâmetro vale o padrão, sem aviso", () => {
    const aviso = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(cenarioDaUrl("")).toBe(CENARIO_PADRAO);
    expect(cenarioDaUrl("?atraso=500")).toBe(CENARIO_PADRAO);
    expect(aviso).not.toHaveBeenCalled();
  });

  it("nome conhecido escolhe o cenário", () => {
    expect(cenarioDaUrl("?cenario=github-err-02&atraso=500")).toBe("github-err-02");
  });

  it("nome desconhecido cai no padrão e mostra a lista no console", () => {
    const aviso = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(cenarioDaUrl("?cenario=nao-existe")).toBe(CENARIO_PADRAO);
    expect(aviso).toHaveBeenCalledOnce();
    const texto = String(aviso.mock.calls[0]![0]);
    expect(texto).toContain('"nao-existe"');
    for (const nome of Object.keys(CENARIOS)) expect(texto).toContain(nome);
  });

  it.each([
    "obsidian-sem-login",
    "obsidian-nao-instalado",
    "github-sem-login",
    "github-nao-instalado",
  ])("%s não existe (o núcleo nunca manda): cai no padrão com aviso", (nome) => {
    const aviso = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(cenarioDaUrl(`?cenario=${nome}`)).toBe(CENARIO_PADRAO);
    expect(aviso).toHaveBeenCalledOnce();
    expect(String(aviso.mock.calls[0]![0])).toContain(`"${nome}" desconhecido`);
  });

  it("nomes em kebab-case", () => {
    for (const nome of Object.keys(CENARIOS)) expect(nome).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/);
  });
});
