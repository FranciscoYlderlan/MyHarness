// Seção Prompts de papel (CMP-16) da TEL-06: espaço reservado, até a E4-T6.
import { SecaoDeConfiguracoes } from "@/features/configuracoes/secao-de-configuracoes";

export function SecaoPrompts() {
  return (
    <SecaoDeConfiguracoes
      id="CMP-16"
      titulo="Prompts de papel"
      descricao="O prompt curto de cada fluxo, com versão, palavras e tokens."
    />
  );
}
