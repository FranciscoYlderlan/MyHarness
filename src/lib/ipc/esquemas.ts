// Schemas zod do que chega do núcleo por evento ou canal (arquitetura, seção 1: "zod nos
// eventos que chegam do núcleo"). Os tipos vêm de `src/bindings/` (gerados do Rust); cada
// schema é anotado com o tipo gerado, e `esquemas.test.ts` confere que os dois são iguais,
// então uma mudança no Rust sem a mudança aqui não compila.
import { z } from "zod";
import type * as B from "@/bindings";

/**
 * Data e hora em ISO 8601 UTC com `Z`, como o núcleo grava (sem deslocamento), com
 * segundos e fração opcional de 1 a 9 casas, igual ao `DataHoraUtc` do Rust.
 */
export const dataHoraUtc: z.ZodType<B.DataHoraUtc> = z.iso
  .datetime({ offset: false, local: false })
  .regex(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/);

const id = z.string().min(1);
const inteiro = z.number().int().min(0);

export const codigoErro = z.enum([
  "CLAUDE_CLI_NAO_INSTALADO",
  "GITHUB_OAUTH_NAO_CONCLUIDO",
  "OBSIDIAN_FECHADO",
  "LOGIN_CLAUDE_CLI_FALHOU",
  "CLAUDE_CLI_DESCONECTADO",
  "GITHUB_DESCONECTADO",
  "OBSIDIAN_DESCONECTADO",
  "LIMITE_DO_PLANO_ATINGIDO",
  "PLANO_SEM_AVAL",
  "DECISAO_SEM_AVAL",
  "MODELO_NAO_DEFINIDO",
  "TAREFAS_TOCAM_MESMOS_ARQUIVOS",
  "ESCRITA_NEGADA_PAPEL",
  "COMANDO_GIT_NEGADO",
  "AGUARDANDO_REGISTRO",
  "WORKTREE_AUSENTE",
  "SUBAGENTE_CAIU",
  "LEITURA_DIRETA_BLOQUEADA",
  "IMPLEMENTADOR_NAO_TESTA",
  "CRITERIO_SEM_PROVA",
  "TETO_DE_VOLTAS",
  "CONCLUSAO_COM_PENDENCIA",
  "BRIEFING_ADVERSARIAL_COM_CONTEXTO",
  "ADVERSARIAL_ANTES_DO_FIM_DA_RODADA",
  "RECURSO_OCUPADO",
  "APLICACAO_FORA_DO_AR",
  "ACHADO_SEM_TIPO",
  "SUGESTAO_SEM_DECISAO",
  "REGISTRO_INCOMPLETO",
  "MODELO_INDISPONIVEL",
  "CONFLITO_DE_MERGE",
  "PASTA_INACESSIVEL",
  "COFRE_DO_PROJETO_NAO_ENCONTRADO",
  "PROMPT_ACIMA_DO_LIMITE",
  "PROMPT_VAZIO",
  "PROMPT_DE_PAPEL_AUSENTE",
  "COFRE_NAO_RESPONDE",
  "COFRE_MEMORIA_OBRIGATORIO",
  "ESCRITA_EM_COFRE_DE_CONSULTA",
  "RESULTADO_SEM_COFRE_DE_ORIGEM",
  "CONSUMO_INDISPONIVEL",
  "FALHA_NO_ENVIO",
  "CAPTURA_INDISPONIVEL",
  "TETO_DE_ESCRITA_ATINGIDO",
  "PROJETO_NOME_INVALIDO",
  "REPOSITORIO_INVALIDO",
  "LIMITE_DE_CONTEXTO_FORA_DA_FAIXA",
  "LIMITE_DO_CHAT_ATINGIDO",
  "TAREFA_INCOMPLETA",
  "STATUS_DE_CONSUMO_DESCONHECIDO",
  "GRAVACAO_DIVERGENTE",
  "LIMITE_DE_CONTEXTO_ABAIXO_DO_MINIMO",
  "INTEGRACAO_FALHA_INESPERADA",
  "GRAVACAO_DESFEITA_PELA_METADE",
]) satisfies z.ZodType<B.CodigoErro>;

export const erroCatalogo = z.object({
  codigo: codigoErro,
  id: z.string().regex(/^ERR-\d{2}$/),
  parametros: z.record(z.string(), z.string()),
}) satisfies z.ZodType<B.ErroCatalogo>;

export const tipoIntegracao = z.enum(["claude_cli", "obsidian_cli", "github"]);
export const statusIntegracao = z.enum(["conectado", "desconectado", "sem_login", "nao_instalado"]);

export const integracao = z.object({
  tipo: tipoIntegracao,
  status: statusIntegracao,
  conta: z.string().nullable(),
  versao: z.string().nullable(),
}) satisfies z.ZodType<B.Integracao>;

export const instrucaoConexao = z.discriminatedUnion("tipo", [
  z.object({ tipo: z.literal("codigo_do_dispositivo"), codigo: z.string(), endereco: z.string() }),
  z.object({ tipo: z.literal("mensagem"), texto: z.string() }),
]) satisfies z.ZodType<B.InstrucaoConexao>;

const statusConsumo = z.enum(["allowed", "allowed_warning", "rejected", "desconhecido"]);
const percentual = z.number().int().min(0).max(100);

export const consumo = z.object({
  semanal_pct: percentual,
  janela_pct: percentual,
  proximo_reinicio: dataHoraUtc,
  semanal_reinicio: dataHoraUtc,
  lida_em: dataHoraUtc,
  origem: z.enum(["evento", "estimado"]),
  janela_status: statusConsumo,
  semanal_status: statusConsumo,
}) satisfies z.ZodType<B.Consumo>;

export const chat = z.object({
  id,
  projeto: id,
  limite_tokens: z.number().int().min(1),
  tokens_usados: inteiro,
  chat_anterior: id.nullable(),
  status: z.enum(["ativo", "pausado", "encerrado", "so_leitura"]),
  continuacoes: inteiro,
}) satisfies z.ZodType<B.Chat>;

export const mensagem = z.object({
  id,
  chat: id,
  autor: z.enum(["humano", "orquestrador", "sistema"]),
  texto: z.string(),
  uso_tokens: inteiro.nullable(),
}) satisfies z.ZodType<B.Mensagem>;

const nomePapel = z.enum(["orquestrador", "funcionalidade", "teste", "adversarial", "pesquisa"]);
// `concluido` no fio = "concluído" do contrato (M-38); todo enum da IPC vai em ASCII.
export const statusExecucao = z.enum(["na_fila", "rodando", "concluido", "falhou", "interrompida"]);

export const execucao = z.object({
  id,
  chat: id,
  tarefa: id.nullable(),
  papel: nomePapel,
  modelo: z.string(),
  prompt_versao: z.number().int().min(1),
  worktree: z.string().nullable(),
  branch: z.string().nullable(),
  status: statusExecucao,
  motivo_fila: z.enum(["teto", "toca_em", "recurso"]).nullable(),
}) satisfies z.ZodType<B.Execucao>;

export const log = z.object({
  id,
  execucao: id,
  comando: z.string(),
  inicio: dataHoraUtc,
  fim: dataHoraUtc.nullable(),
  codigo_saida: z.number().int().nullable(),
  saida_mascarada: z.string().nullable(),
}) satisfies z.ZodType<B.Log>;

export const leitura = z.object({
  id,
  execucao: id,
  caminho: z.string(),
  cofre: id.nullable(),
  origem: z.enum(["cli", "arquivo"]),
  hora: dataHoraUtc,
}) satisfies z.ZodType<B.Leitura>;

export const criterio = z.object({
  status: z.enum(["passou", "falhou", "nao_verificado_presumido"]),
  prova: z.string().nullable(),
}) satisfies z.ZodType<B.Criterio>;

export const relatorio = z.object({
  id,
  execucao: id,
  // `concluido` no fio = "concluído" do contrato (M-38).
  resultado: z.enum(["concluido", "falhou", "bloqueado"]),
  o_que_fez: z.string(),
  arquivos: z.array(z.string()),
  prova: z.string(),
  criterios: z.array(criterio),
  duvidas: z.array(z.string()),
  candidatos_a_nota: z.array(z.string()),
  registrado_em: dataHoraUtc.nullable(),
}) satisfies z.ZodType<B.Relatorio>;

export const pendencia = z.discriminatedUnion("tipo", [
  z.object({ tipo: z.literal("plano_proposto"), plano: id }),
  z.object({ tipo: z.literal("duvida_de_subagente"), execucao: id, texto: z.string() }),
  z.object({ tipo: z.literal("teto_de_voltas"), tarefa: id }),
  z.object({ tipo: z.literal("achados_para_decidir"), quantidade: inteiro }),
]) satisfies z.ZodType<B.Pendencia>;

/** Payload de cada evento global, pela chave de `events` dos bindings. */
export const esquemasDeEvento = {
  integracaoMudou: z.object({ integracao }),
  consumoAtualizado: z.object({ consumo, avisos: z.array(erroCatalogo) }),
  chatMudou: z.object({ chat, avisos: z.array(erroCatalogo) }),
  execucaoMudou: z.object({ execucao }),
  logRegistrado: z.object({ log }),
  leituraRegistrada: z.object({ leitura }),
  pendenciasMudaram: z.object({ chat: id, pendencias: z.array(pendencia) }),
} satisfies {
  integracaoMudou: z.ZodType<B.IntegracaoMudou>;
  consumoAtualizado: z.ZodType<B.ConsumoAtualizado>;
  chatMudou: z.ZodType<B.ChatMudou>;
  execucaoMudou: z.ZodType<B.ExecucaoMudou>;
  logRegistrado: z.ZodType<B.LogRegistrado>;
  leituraRegistrada: z.ZodType<B.LeituraRegistrada>;
  pendenciasMudaram: z.ZodType<B.PendenciasMudaram>;
};

/** Saída ao vivo de uma execução, pelo canal (`Channel`) do comando. */
export const eventoExecucao = z.discriminatedUnion("tipo", [
  z.object({ tipo: z.literal("texto"), texto: z.string() }),
  z.object({ tipo: z.literal("ferramenta"), nome: z.string(), resumo: z.string() }),
  z.object({ tipo: z.literal("mensagem"), mensagem }),
  z.object({ tipo: z.literal("uso_tokens"), tokens_usados: inteiro }),
  z.object({ tipo: z.literal("plano_proposto"), plano: id }),
  z.object({ tipo: z.literal("status"), status: statusExecucao }),
  z.object({ tipo: z.literal("erro"), erro: erroCatalogo }),
]) satisfies z.ZodType<B.EventoExecucao>;
