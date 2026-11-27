// TEL-01 em espaços: um espaço por CMP e o botão Configurações, que abre a TEL-06.
import { act, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mockCommands } from "@/bindings";
import { criarNucleoSimulado } from "@/dev/nucleo-simulado";
import { MainWindow } from "./main-window";

afterEach(() => clearMocks());

function abrir() {
  mockCommands(criarNucleoSimulado().handlers);
  render(<MainWindow />);
}

describe("TEL-01 em espaços", () => {
  it("mostra um espaço por CMP (01, 02, 04, 06, 07, 08, 09) com o ID e o nome", () => {
    abrir();
    for (const nome of [
      "CMP-01 Consumo do plano",
      "CMP-02 Painel de contexto",
      "CMP-04 Terminal com abas por agente",
      "CMP-06 Campo de prompt",
      "CMP-07 Barra do chat e novo chat",
      "CMP-08 Logs ao vivo",
      "CMP-09 Histórico",
    ]) {
      expect(screen.getByRole("region", { name: nome })).toBeInTheDocument();
    }
  });

  it("o espaço do CMP-03 tem o indicador de integrações e o botão Configurações", () => {
    abrir();
    const espaco = document.querySelector<HTMLElement>('[data-cmp="CMP-03"]')!;
    expect(within(espaco).getByRole("button", { name: /^Integrações:/ })).toBeInTheDocument();
    expect(within(espaco).getByRole("button", { name: "Configurações" })).toBeInTheDocument();
  });

  it("Configurações abre a TEL-06 com as seções, e Fechar volta (NAV-15, NAV-16)", async () => {
    abrir();
    act(() => screen.getByRole("button", { name: "Configurações" }).click());
    const tela = await screen.findByRole("dialog", { name: "Configurações" });
    expect(
      within(tela)
        .getAllByRole("heading", { level: 3 })
        .map((h) => h.textContent),
    ).toEqual(["Prompts de papel", "Modelos", "Limites", "Cofres do projeto"]);
    for (const id of ["CMP-16", "CMP-14", "ENT-22", "CMP-17"]) {
      expect(tela.querySelector(`[data-secao="${id}"]`)).not.toBeNull();
    }
    act(() => within(tela).getByRole("button", { name: "Fechar" }).click());
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("o indicador continua abrindo a TEL-02 (NAV-03)", async () => {
    abrir();
    act(() => screen.getByRole("button", { name: /^Integrações:/ }).click());
    expect(await screen.findByRole("dialog", { name: "Integrações" })).toBeInTheDocument();
  });
});
