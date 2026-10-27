//! Mascaramento de segredos (RN-04, RNF-05).
//!
//! Regra, sem adivinhação pelo formato do texto
//! (decisão `geral-decisao-2026-11-20-mascaramento-sem-adivinhar`):
//!
//! 1. Token com prefixo conhecido ([`PREFIXOS`], sem diferenciar maiúsculas) some inteiro:
//!    do prefixo até o primeiro caractere fora de `[A-Za-z0-9_-]`. Qualquer ocorrência
//!    do prefixo inicia um token, sem exigir fronteira antes dele (`%20ghp_…`,
//!    `token%3Dghp_…`, `xghp_…`), desde que venha pelo menos 1 caractere de
//!    `[A-Za-z0-9_-]` depois dele: prefixo nu (`ghs_` sozinho) não é token.
//! 2. O valor exato de cada [`Segredo`] que o núcleo guarda some em toda ocorrência
//!    ([`mascarar_com`]).
//!
//! Os trechos das duas regras são calculados no texto original, juntados quando se
//! tocam ou se sobrepõem e trocados numa passada só, cada trecho unido por um
//! [`MASCARA`]: segredo guardado dentro de um token não deixa sobrar o resto do token.
//!
//! Nada mais é mascarado: cabeçalho `Authorization`, `chave=valor`, `--token`, URL ou
//! PEM ficam como estão, a menos que tragam um desses dois casos. Texto sem segredo sai
//! igual e sem cópia (`Cow::Borrowed`).

use std::borrow::Cow;
use std::fmt;
use std::ops::Range;

/// O que entra no lugar de cada segredo.
pub const MASCARA: &str = "••••";

/// Prefixos de token conhecidos: GitHub e chaves da Anthropic.
pub const PREFIXOS: [&str; 7] = [
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "sk-ant-",
];

/// Mascara os tokens com prefixo conhecido (regra 1).
pub fn mascarar(texto: &str) -> Cow<'_, str> {
    substituir(texto, &trechos_com_prefixo(texto))
}

/// Mascara o valor exato de cada segredo guardado (regra 2) e os tokens com prefixo
/// conhecido (regra 1), numa passada só sobre o texto original.
///
/// Segredo vazio é ignorado. Os trechos de token e de segredo guardado que se tocam ou
/// se sobrepõem (um dentro do outro, encavalados ou colados) viram uma máscara só:
/// nenhum pedaço de nenhum deles sobra.
pub fn mascarar_com<'a>(texto: &'a str, segredos: &[&Segredo]) -> Cow<'a, str> {
    let mut trechos = trechos_guardados(texto, segredos);
    trechos.extend(trechos_com_prefixo(texto));
    substituir(texto, &juntar(trechos))
}

/// Valor secreto que o núcleo guarda (ex.: token do device flow do GitHub).
/// `Debug` mostra `Segredo(••••)` e `Display` mostra `••••`; o valor só sai por
/// [`Segredo::expor`].
#[derive(Clone, PartialEq, Eq)]
pub struct Segredo(String);

impl Segredo {
    /// Guarda o valor.
    pub fn new(valor: impl Into<String>) -> Self {
        Self(valor.into())
    }

    /// O valor em texto puro, para quem precisa dele (ex.: a chamada HTTP).
    /// Nunca passe o resultado a log, SQLite, cofre ou tela.
    pub fn expor(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Segredo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Segredo")
            .field(&format_args!("{MASCARA}"))
            .finish()
    }
}

impl fmt::Display for Segredo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(MASCARA)
    }
}

/// Texto livre vindo de fora (ex.: mensagem de uma CLI), que só se constrói passando pela
/// máscara. `Display` e `Debug` mostram o texto já mascarado; o original não fica guardado.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TextoMascarado(String);

impl TextoMascarado {
    /// Mascara os tokens com prefixo conhecido de `texto`.
    pub fn new(texto: &str) -> Self {
        Self(mascarar(texto).into_owned())
    }

    /// Mascara os segredos guardados e os tokens com prefixo conhecido de `texto`.
    pub fn com_segredos(texto: &str, segredos: &[&Segredo]) -> Self {
        Self(mascarar_com(texto, segredos).into_owned())
    }

    /// O texto já mascarado.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for TextoMascarado {
    fn from(texto: &str) -> Self {
        Self::new(texto)
    }
}

impl From<String> for TextoMascarado {
    fn from(texto: String) -> Self {
        match mascarar(&texto) {
            Cow::Owned(m) => Self(m),
            Cow::Borrowed(_) => Self(texto),
        }
    }
}

impl fmt::Display for TextoMascarado {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for TextoMascarado {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

/// Troca cada trecho (em ordem e sem sobreposição) por [`MASCARA`].
fn substituir<'a>(texto: &'a str, trechos: &[Range<usize>]) -> Cow<'a, str> {
    if trechos.is_empty() {
        return Cow::Borrowed(texto);
    }
    let mut saida = String::with_capacity(texto.len());
    let mut cursor = 0;
    for trecho in trechos {
        saida.push_str(&texto[cursor..trecho.start]);
        saida.push_str(MASCARA);
        cursor = trecho.end;
    }
    saida.push_str(&texto[cursor..]);
    Cow::Owned(saida)
}

/// Trechos dos tokens com prefixo conhecido. Os prefixos e o corpo do token são ASCII,
/// então todo índice devolvido cai em fronteira de caractere UTF-8.
fn trechos_com_prefixo(texto: &str) -> Vec<Range<usize>> {
    let bytes = texto.as_bytes();
    let de_token = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    let mut trechos = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // Prefixo seguido de pelo menos 1 caractere de token; sem fronteira antes dele.
        let prefixo = PREFIXOS.iter().find(|p| {
            bytes
                .get(i..i + p.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(p.as_bytes()))
                && bytes.get(i + p.len()).is_some_and(|&b| de_token(b))
        });
        match prefixo {
            Some(p) => {
                let mut fim = i + p.len();
                while fim < bytes.len() && de_token(bytes[fim]) {
                    fim += 1;
                }
                trechos.push(i..fim);
                i = fim;
            }
            None => i += 1,
        }
    }
    trechos
}

/// Trechos de todas as ocorrências (inclusive encavaladas) dos valores guardados,
/// já juntados.
fn trechos_guardados(texto: &str, segredos: &[&Segredo]) -> Vec<Range<usize>> {
    let mut trechos = Vec::new();
    for valor in segredos.iter().map(|s| s.expor()).filter(|v| !v.is_empty()) {
        let mut desde = 0;
        while let Some(pos) = texto[desde..].find(valor) {
            let inicio = desde + pos;
            trechos.push(inicio..inicio + valor.len());
            desde = inicio + texto[inicio..].chars().next().map_or(1, char::len_utf8);
        }
    }
    juntar(trechos)
}

/// Ordena os trechos e junta os que se tocam ou se sobrepõem, para [`substituir`].
fn juntar(mut trechos: Vec<Range<usize>>) -> Vec<Range<usize>> {
    // Do início para o fim e, no mesmo início, o mais longo primeiro.
    trechos.sort_by_key(|t| (t.start, std::cmp::Reverse(t.end)));
    let mut juntos: Vec<Range<usize>> = Vec::with_capacity(trechos.len());
    for t in trechos {
        match juntos.last_mut() {
            Some(ultimo) if t.start <= ultimo.end => ultimo.end = ultimo.end.max(t.end),
            _ => juntos.push(t),
        }
    }
    juntos
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPO: &str = "A1b2C3d4E5f6G7h8I9j0KlMnOpQrStUvWx";

    fn token(prefixo: &str) -> String {
        format!("{prefixo}{CORPO}")
    }

    fn sem_mudanca(texto: &str) {
        assert!(
            matches!(mascarar(texto), Cow::Borrowed(t) if t == texto),
            "{texto:?}"
        );
    }

    #[test]
    fn mascara_e_os_quatro_pontos() {
        assert_eq!(MASCARA, "••••");
    }

    #[test]
    fn cada_prefixo_some_inteiro_no_meio_de_frase() {
        for p in PREFIXOS {
            let t = token(p);
            assert_eq!(
                mascarar(&format!("falhou com {t} agora")),
                "falhou com •••• agora",
                "{p}"
            );
        }
    }

    #[test]
    fn cada_prefixo_em_varias_ocorrencias() {
        for p in PREFIXOS {
            let t = token(p);
            let entrada = format!("{t} e {t}, de novo {t}");
            let saida = mascarar(&entrada);
            assert_eq!(saida, "•••• e ••••, de novo ••••", "{p}");
        }
    }

    #[test]
    fn prefixo_em_maiusculas_ou_misturado() {
        for p in PREFIXOS {
            for variante in [p.to_uppercase(), {
                let mut s = p.to_string();
                s.replace_range(..1, &s[..1].to_uppercase());
                s
            }] {
                let t = token(&variante);
                assert_eq!(mascarar(&format!("x {t} y")), "x •••• y", "{variante}");
            }
        }
    }

    #[test]
    fn colado_em_pontuacao_e_aspas() {
        for p in PREFIXOS {
            let t = token(p);
            for (entrada, esperado) in [
                (format!("({t})"), "(••••)".to_string()),
                (format!("\"{t}\""), "\"••••\"".to_string()),
                (format!("'{t}'."), "'••••'.".to_string()),
                (format!("`{t}`;"), "`••••`;".to_string()),
                (format!("token={t}&x=1"), "token=••••&x=1".to_string()),
                (format!("<{t}>"), "<••••>".to_string()),
                (format!("{t}\n{t}"), "••••\n••••".to_string()),
                (format!("→{t}é"), "→••••é".to_string()),
            ] {
                assert_eq!(mascarar(&entrada), esperado, "{entrada:?}");
            }
        }
    }

    #[test]
    fn dentro_de_json() {
        let t = token("github_pat_");
        let k = token("sk-ant-api03-");
        let json = format!(r#"{{"token":"{t}","chave":"{k}","lista":["{t}"]}}"#);
        assert_eq!(
            mascarar(&json),
            r#"{"token":"••••","chave":"••••","lista":["••••"]}"#
        );
    }

    #[test]
    fn prefixo_sem_fronteira_tambem_conta() {
        for (entrada, esperado) in [
            ("xghp_abc", "x••••"),
            ("1ghp_abc", "1••••"),
            ("_ghp_abc", "_••••"),
            ("meu_ghp_abc", "meu_••••"),
            ("meu_github_pat_abc", "meu_••••"),
            ("desk-ant-abc", "de••••"),
            ("a-ghp_abc", "a-••••"),
        ] {
            assert_eq!(mascarar(entrada), esperado, "{entrada:?}");
        }
        for texto in ["ghx_abc", "sk-anthropic"] {
            sem_mudanca(texto);
        }
    }

    #[test]
    fn prefixo_colado_em_codificacao_de_url() {
        let t = "ghp_16C7e42F292c6912E7710c838347Ae178B4a";
        assert_eq!(mascarar(&format!("%20{t}%20")), "%20••••%20");
        assert_eq!(mascarar(&format!("token%3D{t}")), "token%3D••••");
    }

    #[test]
    fn prefixo_nu_nao_e_token() {
        sem_mudanca("ghs-abc não é ghs_");
        for p in PREFIXOS {
            sem_mudanca(p);
            sem_mudanca(&format!("{p} solto"));
        }
        assert_eq!(mascarar("ghp_x"), "••••");
    }

    #[test]
    fn token_termina_no_primeiro_caractere_fora_do_conjunto() {
        assert_eq!(mascarar("ghp_ab-c_d.e"), "••••.e");
        assert_eq!(mascarar("ghp_ab/cd"), "••••/cd");
        assert_eq!(mascarar("ghp_-"), "••••");
    }

    #[test]
    fn segredo_guardado_dentro_de_token_nao_vaza_o_resto() {
        let entrada = "x ghp_16C7e42F292c6912E7710c838347Ae178B4a y";
        for valor in ["292c6912", "ghp_16C7e42F"] {
            let s = Segredo::new(valor);
            assert_eq!(mascarar_com(entrada, &[&s]), "x •••• y", "{valor:?}");
        }
    }

    #[test]
    fn segredo_colado_em_token_vira_uma_mascara_so() {
        let s = Segredo::new("guardado");
        assert_eq!(mascarar_com("x guardadoghp_abc y", &[&s]), "x •••• y");
        assert_eq!(mascarar_com("x ghp_abc.guardado y", &[&s]), "x ••••.•••• y");
    }

    #[test]
    fn idempotente() {
        let t = token("ghp_");
        let s = Segredo::new("senha-guardada");
        let entrada = format!("{t} senha-guardada \"{t}\"");
        let uma = mascarar_com(&entrada, &[&s]).into_owned();
        assert_eq!(mascarar_com(&uma, &[&s]), uma);
        assert_eq!(mascarar(&uma), uma);
    }

    #[test]
    fn prosa_e_mensagens_de_cli_ficam_iguais() {
        for texto in [
            "",
            "token: expirado há pouco tempo",
            "A senha está errada; tente de novo.",
            "Authorization: Bearer abc.def.ghi",
            "token=abc123 password=hunter2",
            "https://usuario:senha@exemplo.com/caminho",
            "-----BEGIN RSA PRIVATE KEY-----\nMIIabc\n-----END RSA PRIVATE KEY-----",
            "<token>abc</token>",
            "error: the argument '--token <TOKEN>' requires a value but none was supplied",
            "error: unexpected argument '--tokn' found\n\n  tip: a similar argument exists: '--token'",
            "The CLI is unable to find Obsidian",
            "Invalid API key · Please run /login",
            "Não foi possível encontrar o Obsidian. Abra o app e tente de novo.",
            "Falha no login: tempo esgotado (30 s).",
            "ghost_user saiu; sk-8 é outra coisa; gh pr list",
        ] {
            sem_mudanca(texto);
        }
    }

    #[test]
    fn valor_guardado_em_qualquer_posicao_e_repetido() {
        let s = Segredo::new("v4lor-s3m-formato");
        let segredos = [&s];
        for (entrada, esperado) in [
            ("v4lor-s3m-formato", "••••"),
            ("v4lor-s3m-formato no início", "•••• no início"),
            ("no fim v4lor-s3m-formato", "no fim ••••"),
            ("colado:v4lor-s3m-formato!", "colado:••••!"),
            ("dentroXv4lor-s3m-formatoY", "dentroX••••Y"),
            ("v4lor-s3m-formato e v4lor-s3m-formato", "•••• e ••••"),
        ] {
            assert_eq!(mascarar_com(entrada, &segredos), esperado, "{entrada:?}");
        }
    }

    #[test]
    fn segredos_sobrepostos_somem_inteiros() {
        let longo = Segredo::new("abcdef");
        let curto = Segredo::new("cde");
        // um dentro do outro, em qualquer ordem
        assert_eq!(mascarar_com("x abcdef y", &[&curto, &longo]), "x •••• y");
        assert_eq!(
            mascarar_com("x abcdef y cde", &[&longo, &curto]),
            "x •••• y ••••"
        );
        // encavalados: nenhum pedaço sobra
        let a = Segredo::new("abcd");
        let b = Segredo::new("cdef");
        assert_eq!(mascarar_com("[abcdef]", &[&a, &b]), "[••••]");
        // ocorrências encavaladas do mesmo segredo
        assert_eq!(mascarar_com("aaa", &[&Segredo::new("aa")]), "••••");
    }

    #[test]
    fn segredo_vazio_e_ignorado_e_sem_segredo_nao_copia() {
        let vazio = Segredo::new("");
        assert!(matches!(
            mascarar_com("nada", &[&vazio]),
            Cow::Borrowed("nada")
        ));
        assert!(matches!(mascarar_com("nada", &[]), Cow::Borrowed("nada")));
    }

    #[test]
    fn mascarar_com_tambem_aplica_os_prefixos() {
        let s = Segredo::new("guardado");
        let t = token("gho_");
        assert_eq!(
            mascarar_com(&format!("guardado e {t}"), &[&s]),
            "•••• e ••••"
        );
        // segredo guardado que contém um token com prefixo some inteiro
        let composto = Segredo::new(format!("id {t} fim"));
        assert_eq!(
            mascarar_com(&format!("<id {t} fim>"), &[&composto]),
            "<••••>"
        );
    }

    #[test]
    fn segredo_nao_mostra_o_valor() {
        let s = Segredo::new(token("ghp_"));
        assert_eq!(s.to_string(), "••••");
        assert_eq!(format!("{s:?}"), "Segredo(••••)");
        assert_eq!(format!("{s:#?}"), "Segredo(\n    ••••,\n)");
        assert_eq!(s.expor(), token("ghp_"));
    }

    #[test]
    fn texto_mascarado_so_guarda_o_mascarado() {
        let t = token("ghs_");
        let m = TextoMascarado::from(format!("erro {t}"));
        assert_eq!(m.as_str(), "erro ••••");
        assert_eq!(format!("{m:?}"), "\"erro ••••\"");
        assert_eq!(
            TextoMascarado::from("sem nada"),
            TextoMascarado::new("sem nada")
        );
        let s = Segredo::new("guardado");
        assert_eq!(
            TextoMascarado::com_segredos("x guardado", &[&s]).to_string(),
            "x ••••"
        );
    }
}
