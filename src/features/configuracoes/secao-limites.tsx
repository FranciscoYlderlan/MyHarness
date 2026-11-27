// Seção Limites da TEL-06: os parâmetros da ENT-22 (tabela de parâmetros, lacuna aprovada da etapa 4); espaço reservado, até a E4-T6.
import { SecaoDeConfiguracoes } from "@/features/configuracoes/secao-de-configuracoes";

export function SecaoLimites() {
  return (
    <SecaoDeConfiguracoes
      id="ENT-22"
      titulo="Limites"
      descricao="Os parâmetros numéricos do MyHarness, com faixa e padrão de fábrica."
    />
  );
}
