// Testes de componente da TEL-02 e do CMP-03 contra o núcleo simulado das integrações,
// pelo mesmo contrato IPC (`mockCommands` dos bindings).
import { act, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { clearMocks } from "@tauri-apps/api/mocks";
import { MainWindow } from "@/app/main-window";
import { mockCommands } from "@/bindings";
import type { IntegracaoMudou } from "@/lib/ipc";
import { estadoInicial, reduzir } from "./estado";
import {
  criarNucleoSimuladoDeIntegracoes,
  erroDoCatalogo,
  falhaInesperada,
  type NucleoSimuladoDeIntegracoes,
} from "@/lib/ipc/nucleo-simulado-integracoes";

afterEach(() => clearMocks());

function iniciar(
  inicial: Parameters<typeof criarNucleoSimuladoDeIntegracoes>[0],
): NucleoSimuladoDeIntegracoes {
  const nucleo = criarNucleoSimuladoDeIntegracoes(inicial);
  mockCommands(nucleo.handlers);
  render(<MainWindow />);
  return nucleo;
}

// `hidden`: com o painel aberto, o Radix esconde o resto da tela dos leitores de tela.
const indicador = () => screen.getByRole("button", { name: /^Integrações:/, hidden: true });

async function abrirPainel() {
  await waitFor(() => expect(indicador()).not.toHaveAttribute("data-situacao", "carregando"));
  act(() => indicador().click());
  const painel = await screen.findByRole("dialog", { name: "Integrações" });
  await waitFor(() => expect(within(painel).queryAllByText("checando")).toHaveLength(0));
  return painel;
}

const cartao = (nome: string) => screen.getByRole("article", { name: nome });

const todasConectadas = {
  claude_cli: { status: "conectado", versao: "2.1.0 (Claude Code)" },
  obsidian_cli: { status: "conectado" },
  github: { status: "conectado", conta: "FranciscoYlderlan" },
} as const;

describe("CMP-03 indicador de integrações", () => {
  it("mostra checando, depois todas conectadas", async () => {
    iniciar(todasConectadas);
    expect(indicador()).toHaveAttribute("data-situacao", "carregando");
    await waitFor(() => expect(indicador()).toHaveAttribute("data-situacao", "conectado"));
    expect(indicador()).toHaveAccessibleName(/todas conectadas/);
  });

  it("destaca a integração desconectada e acompanha o evento integracao-mudou", async () => {
    iniciar({ ...todasConectadas, github: { status: "desconectado" } });
    await waitFor(() => expect(indicador()).toHaveAttribute("data-situacao", "erro"));
    expect(indicador()).toHaveAccessibleName(/GitHub: desconectado/);

    const mudou: IntegracaoMudou = {
      integracao: {
        tipo: "github",
        status: "conectado",
        conta: "octo",
        versao: null,
        notas_indexadas: null,
        erro: null,
      },
    };
    await act(() => emit("integracao-mudou", mudou));
    await waitFor(() => expect(indicador()).toHaveAttribute("data-situacao", "conectado"));
  });

  it("AC-03: Claude CLI ausente deixa o indicador em erro e o cartão com o link (ERR-01)", async () => {
    iniciar({ ...todasConectadas, claude_cli: { status: "nao_instalado" } });
    await waitFor(() => expect(indicador()).toHaveAttribute("data-situacao", "erro"));
    await abrirPainel();
    const c = cartao("Claude CLI");
    expect(within(c).getByRole("alert")).toHaveTextContent("Claude CLI não encontrado");
    expect(within(c).getByRole("link", { name: "Instalar o Claude CLI" })).toHaveAttribute(
      "href",
      "https://docs.claude.com/en/docs/claude-code/setup",
    );
    expect(within(c).getByRole("button", { name: "Testar" })).toBeInTheDocument();
    expect(within(c).queryByRole("button", { name: "Conectar" })).not.toBeInTheDocument();
  });

  it("sem núcleo (tela fora do app) mostra o ERR-55 em vez de ficar checando", async () => {
    render(<MainWindow />);
    await waitFor(() => expect(indicador()).toHaveAttribute("data-situacao", "erro"));
    act(() => indicador().click());
    const c = await screen.findByRole("article", { name: "GitHub" });
    await waitFor(() =>
      expect(within(c).getByRole("alert")).toHaveTextContent(
        "Algo deu errado no MyHarness: o núcleo não respondeu. Tente de novo.",
      ),
    );
  });
});

describe("TEL-02 integrações", () => {
  it("AC-91: GitHub desconectado aparece em erro com o botão Conectar", async () => {
    iniciar({ ...todasConectadas, github: { status: "desconectado" } });
    await abrirPainel();
    const c = cartao("GitHub");
    expect(c).toHaveAttribute("data-situacao", "erro");
    expect(c).toHaveTextContent("desconectado");
    expect(within(c).getByRole("button", { name: "Conectar" })).toBeEnabled();
  });

  it("AC-153: rede fora do ar vira ERR-53 com conectar de novo; com a rede de volta, conecta", async () => {
    const nucleo = iniciar(todasConectadas);
    await waitFor(() => expect(indicador()).toHaveAttribute("data-situacao", "conectado"));

    nucleo.falharChecagem("github", falhaInesperada("github", "erro de rede: conexão recusada"));
    await abrirPainel();
    const c = cartao("GitHub");
    expect(c).toHaveAttribute("data-situacao", "erro");
    expect(c).toHaveTextContent("desconectado");
    expect(within(c).getByRole("alert")).toHaveTextContent(
      "Falha inesperada com GitHub: erro de rede: conexão recusada. Tente de novo.",
    );
    expect(indicador()).toHaveAttribute("data-situacao", "erro");

    // Rede de volta: conectar de novo funciona (device flow).
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
    act(() => within(c).getByRole("button", { name: "Conectar" }).click());
    await waitFor(() => expect(c).toHaveTextContent("login em andamento"));
    expect(within(c).getByText("WDJB-MJHT")).toBeInTheDocument();
    expect(within(c).getByRole("link", { name: "github.com/login/device" })).toHaveAttribute(
      "href",
      "https://github.com/login/device",
    );
    expect(within(c).queryByRole("button")).not.toBeInTheDocument();

    act(() => nucleo.concluirConexao("github", { conta: "FranciscoYlderlan" }));
    await waitFor(() => expect(c).toHaveAttribute("data-situacao", "conectado"));
    expect(c).toHaveTextContent("FranciscoYlderlan");
    expect(within(c).queryByRole("alert")).not.toBeInTheDocument();
    expect(within(c).getByRole("button", { name: "Desconectar" })).toBeInTheDocument();
    await waitFor(() => expect(indicador()).toHaveAttribute("data-situacao", "conectado"));
  });

  it("AC-02 e estado conectado: mostra conta e versão, com Testar e Desconectar", async () => {
    iniciar(todasConectadas);
    await abrirPainel();
    const claude = cartao("Claude CLI");
    expect(claude).toHaveAttribute("data-situacao", "conectado");
    expect(claude).toHaveTextContent("2.1.0 (Claude Code)");
    expect(within(claude).queryByRole("button", { name: "Conectar" })).not.toBeInTheDocument();
    expect(within(claude).getByRole("button", { name: "Testar" })).toBeInTheDocument();
    expect(within(claude).getByRole("button", { name: "Desconectar" })).toBeInTheDocument();
  });

  it("Desconectar deixa a integração desconectada com Conectar", async () => {
    iniciar(todasConectadas);
    await abrirPainel();
    const c = cartao("GitHub");
    act(() => within(c).getByRole("button", { name: "Desconectar" }).click());
    await waitFor(() => expect(c).toHaveAttribute("data-situacao", "erro"));
    expect(c).toHaveTextContent("desconectado");
    expect(within(c).getByRole("button", { name: "Conectar" })).toBeInTheDocument();
  });

  it("Desconectar com falha inesperada mostra o ERR-53 e Repetir", async () => {
    const nucleo = iniciar(todasConectadas);
    await abrirPainel();
    nucleo.falharDesconexao("github", falhaInesperada("github", "tempo esgotado"));
    const c = cartao("GitHub");
    act(() => within(c).getByRole("button", { name: "Desconectar" }).click());
    await waitFor(() =>
      expect(within(c).getByRole("alert")).toHaveTextContent(
        "Falha inesperada com GitHub: tempo esgotado. Tente de novo.",
      ),
    );
    act(() => within(c).getByRole("button", { name: "Repetir" }).click());
    await waitFor(() => expect(within(c).queryByRole("alert")).not.toBeInTheDocument());
    expect(c).toHaveTextContent("desconectado");
  });

  it("AC-88: Claude CLI sem login, o /login mostra as mensagens e conclui conectado", async () => {
    const nucleo = iniciar({ ...todasConectadas, claude_cli: { status: "sem_login" } });
    await abrirPainel();
    const c = cartao("Claude CLI");
    expect(c).toHaveTextContent("sem login");
    nucleo.roteiroDeConexao("claude_cli", {
      instrucoes: [{ tipo: "mensagem", texto: "Abrindo o navegador para o login…" }],
      aguardarHumano: true,
    });
    act(() => within(c).getByRole("button", { name: "Conectar" }).click());
    await waitFor(() =>
      expect(within(c).getByText("Abrindo o navegador para o login…")).toBeInTheDocument(),
    );
    act(() => nucleo.concluirConexao("claude_cli", { versao: "2.1.0 (Claude Code)" }));
    await waitFor(() => expect(c).toHaveAttribute("data-situacao", "conectado"));
    expect(c).toHaveTextContent("2.1.0 (Claude Code)");
  });

  it("ERR-04: login do Claude CLI que falha mostra o motivo e Repetir", async () => {
    const nucleo = iniciar({ ...todasConectadas, claude_cli: { status: "sem_login" } });
    await abrirPainel();
    const c = cartao("Claude CLI");
    nucleo.falharConexaoDireto(
      "claude_cli",
      erroDoCatalogo("LOGIN_CLAUDE_CLI_FALHOU", "ERR-04", {
        integracao: "Claude CLI",
        motivo: "tempo esgotado esperando o login",
      }),
    );
    act(() => within(c).getByRole("button", { name: "Conectar" }).click());
    await waitFor(() =>
      expect(within(c).getByRole("alert")).toHaveTextContent("tempo esgotado esperando o login"),
    );
    expect(c).toHaveTextContent("sem login");
    expect(within(c).queryByRole("button", { name: "Conectar" })).not.toBeInTheDocument();
    act(() => within(c).getByRole("button", { name: "Repetir" }).click());
    await waitFor(() => expect(c).toHaveAttribute("data-situacao", "conectado"));
  });

  it("AC-05 e AC-89: Obsidian fechado mostra a mensagem da CLI e Abrir Obsidian (ERR-03)", async () => {
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
    const c = cartao("Obsidian CLI");
    expect(c).toHaveAttribute("data-situacao", "erro");
    expect(c).toHaveTextContent("desconectado");
    expect(within(c).getByRole("alert")).toHaveTextContent(
      "The CLI is unable to find Obsidian. Make sure Obsidian is running.",
    );
    expect(within(c).getByRole("link", { name: "Abrir Obsidian" })).toHaveAttribute(
      "href",
      "obsidian://open",
    );
    expect(within(c).getByRole("button", { name: "Conectar" })).toBeInTheDocument();
  });

  it("AC-04 e AC-129: device flow recusado mostra o motivo e o GitHub segue desconectado (ERR-02)", async () => {
    const nucleo = iniciar({ ...todasConectadas, github: { status: "desconectado" } });
    await abrirPainel();
    const c = cartao("GitHub");
    nucleo.roteiroDeConexao("github", {
      instrucoes: [
        {
          tipo: "codigo_do_dispositivo",
          codigo: "ABCD-1234",
          endereco: "https://github.com/login/device",
        },
      ],
      aguardarHumano: true,
    });
    act(() => within(c).getByRole("button", { name: "Conectar" }).click());
    await waitFor(() => expect(within(c).getByText("ABCD-1234")).toBeInTheDocument());
    act(() =>
      nucleo.falharConexao(
        "github",
        erroDoCatalogo("GITHUB_OAUTH_NAO_CONCLUIDO", "ERR-02", {
          integracao: "GitHub",
          motivo: "access_denied",
        }),
      ),
    );
    await waitFor(() =>
      expect(within(c).getByRole("alert")).toHaveTextContent(
        "Conexão com o GitHub não concluída (recusada no GitHub)",
      ),
    );
    expect(c).toHaveTextContent("desconectado");
    expect(within(c).queryByText("ABCD-1234")).not.toBeInTheDocument();
    expect(within(c).getByRole("button", { name: "Conectar" })).toBeInTheDocument();
  });

  it("AC-127: OAuth concluído mostra a conta no cartão", async () => {
    const nucleo = iniciar({ ...todasConectadas, github: { status: "desconectado" } });
    await abrirPainel();
    const c = cartao("GitHub");
    // Sem roteiro pendente, o núcleo simulado conclui na hora.
    nucleo.definir("github", { conta: "octo" });
    act(() => within(c).getByRole("button", { name: "Conectar" }).click());
    await waitFor(() => expect(c).toHaveAttribute("data-situacao", "conectado"));
    await waitFor(() => expect(c).toHaveTextContent("octo"));
  });

  it("comando ainda não implementado no núcleo mostra o ERR-55 com o motivo", async () => {
    const nucleo = iniciar({ ...todasConectadas, github: { status: "desconectado" } });
    await abrirPainel();
    nucleo.falharConexaoDireto(
      "github",
      erroDoCatalogo("FALHA_INTERNA", "ERR-55", {
        motivo: "o comando conectar_integracao ainda não foi implementado",
      }),
    );
    const c = cartao("GitHub");
    act(() => within(c).getByRole("button", { name: "Conectar" }).click());
    await waitFor(() =>
      expect(within(c).getByRole("alert")).toHaveTextContent(
        "Algo deu errado no MyHarness: o comando conectar_integracao ainda não foi implementado. Tente de novo.",
      ),
    );
  });

  it("AC-128: Obsidian aberto mostra o número de notas indexadas no cartão", async () => {
    iniciar({ ...todasConectadas, obsidian_cli: { status: "conectado", notas_indexadas: 75 } });
    await abrirPainel();
    const c = cartao("Obsidian CLI");
    expect(c).toHaveAttribute("data-situacao", "conectado");
    expect(within(c).getByText("Notas indexadas")).toBeInTheDocument();
    expect(within(c).getByText("75")).toBeInTheDocument();
    expect(within(cartao("GitHub")).queryByText("Notas indexadas")).not.toBeInTheDocument();
  });

  it("M-46: o motivo do ERR-53 que vem de listar aparece no cartão, com Repetir", async () => {
    const erro = falhaInesperada("obsidian_cli", "cofre myHarness-vault não encontrado");
    if (erro.tipo !== "catalogo") throw new Error("erro do catálogo esperado");
    iniciar({ ...todasConectadas, obsidian_cli: { status: "desconectado", erro: erro.erro } });
    await waitFor(() => expect(indicador()).toHaveAttribute("data-situacao", "erro"));
    expect(indicador()).toHaveAccessibleName(/Obsidian CLI: desconectado/);
    await abrirPainel();
    const c = cartao("Obsidian CLI");
    expect(within(c).getByRole("alert")).toHaveTextContent(
      "Falha inesperada com Obsidian CLI: cofre myHarness-vault não encontrado. Tente de novo.",
    );
    expect(within(c).getByRole("button", { name: "Repetir" })).toBeInTheDocument();
  });

  it("listar com ERR-01 mostra não instalado e o link de instalação", async () => {
    iniciar({
      ...todasConectadas,
      claude_cli: {
        status: "nao_instalado",
        erro: { codigo: "CLAUDE_CLI_NAO_INSTALADO", id: "ERR-01", parametros: {} },
      },
    });
    await abrirPainel();
    const c = cartao("Claude CLI");
    expect(c).toHaveTextContent("não instalado");
    expect(within(c).getByRole("alert")).toHaveTextContent("Claude CLI não encontrado");
    expect(within(c).getByRole("link", { name: "Instalar o Claude CLI" })).toBeInTheDocument();
  });

  it("fechar o painel volta à área de trabalho com o indicador atualizado (NAV-04)", async () => {
    iniciar({ ...todasConectadas, github: { status: "desconectado" } });
    const painel = await abrirPainel();
    act(() => within(painel).getByRole("button", { name: "Conectar" }).click());
    await waitFor(() => expect(cartao("GitHub")).toHaveAttribute("data-situacao", "conectado"));
    act(() => within(painel).getByRole("button", { name: "Fechar" }).click());
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(indicador()).toHaveAttribute("data-situacao", "conectado");
  });
});

describe("estado da TEL-02", () => {
  it("M-46: o erro que vem no estado (listar ou evento) vira o erro do cartão", () => {
    const erro = falhaInesperada("github", "erro de rede");
    if (erro.tipo !== "catalogo") throw new Error("erro do catálogo esperado");
    const estado = reduzir(estadoInicial(), {
      tipo: "recebeu",
      integracao: {
        tipo: "github",
        status: "desconectado",
        conta: null,
        versao: null,
        notas_indexadas: null,
        erro: erro.erro,
      },
    });
    expect(estado.github.erro).toEqual({ erro: erro.erro, operacao: "checar" });
    const limpo = reduzir(estado, {
      tipo: "recebeu",
      integracao: { ...estado.github.integracao!, status: "conectado", erro: null },
    });
    expect(limpo.github.erro).toBeNull();
  });
});
