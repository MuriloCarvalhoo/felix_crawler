<div align="center">

# Felix

**Contas a receber de órgãos públicos: do PDF do empenho à minuta de cobrança.**

Extrai dados de Notas de Empenho do SIAFI, controla o saldo de cada empenho, calcula vencimentos e atrasos das notas fiscais e gera a minuta do ofício de cobrança.

![Rust](https://img.shields.io/badge/Rust-1.96-orange?logo=rust)
![Edition](https://img.shields.io/badge/edition-2024-blue)
![unsafe](https://img.shields.io/badge/unsafe-forbidden-success)
![Sem IA](https://img.shields.io/badge/extração-sem%20IA-informational)
![Status](https://img.shields.io/badge/status-MVP%20single--tenant-yellow)

[Regras de negócio](#-regras-de-negócio) ·
[Como funciona](#-como-funciona) ·
[Início rápido](#-início-rápido) ·
[API](#-api) ·
[Roadmap](#-roadmap)

</div>

---

## Sumário

- [Por que o Felix existe](#-por-que-o-felix-existe)
- [Como funciona](#-como-funciona)
- [Regras de negócio](#-regras-de-negócio)
  - [1. Empresa e acesso](#1-empresa-e-acesso)
  - [2. Leitura do PDF do empenho](#2-leitura-do-pdf-do-empenho)
  - [3. Cadastro do empenho](#3-cadastro-do-empenho)
  - [4. Notas fiscais e saldo](#4-notas-fiscais-e-saldo)
  - [5. Vencimento, atraso e encargos](#5-vencimento-atraso-e-encargos)
  - [6. Minuta do ofício de cobrança](#6-minuta-do-ofício-de-cobrança)
  - [7. O que o sistema não faz](#7-o-que-o-sistema-não-faz)
- [Glossário](#-glossário)
- [Início rápido](#-início-rápido)
- [Configuração](#-configuração)
- [API](#-api)
- [Limites operacionais](#-limites-operacionais)
- [Arquitetura](#-arquitetura)
- [Desenvolvimento](#-desenvolvimento)
- [Deploy](#-deploy)
- [Roadmap](#-roadmap)
- [Documentação](#-documentação)

---

## 💡 Por que o Felix existe

Quem vende para o governo recebe uma **Nota de Empenho** (o compromisso do órgão de pagar), emite **notas fiscais** contra ela e depois espera o pagamento — que muitas vezes atrasa. O controle costuma ficar em planilhas: quanto já foi faturado de cada empenho, quanto ainda falta, qual nota venceu e quanto cobrar de multa e juros.

O Felix substitui essa planilha:

| Problema | O que o Felix faz |
|---|---|
| Digitar os dados do empenho à mão | Lê o PDF do SIAFI e preenche UG, número, CNPJ, valor, modalidade e edital |
| Faturar acima do valor empenhado | Bloqueia qualquer nota que ultrapasse o saldo do empenho |
| Não saber o que está vencido | Calcula vencimento e dias de atraso de cada nota |
| Montar a cobrança do zero | Gera a minuta do ofício com principal, multa, juros e base legal |

> **Importante:** o Felix **não usa inteligência artificial**. A extração é feita por regras fixas sobre o texto do PDF. Se um campo não for encontrado, ele fica em branco e o sistema avisa — **nada é inventado**.

---

## 🔄 Como funciona

```mermaid
flowchart LR
    A[📄 PDF da Nota de Empenho<br/>SIAFI] --> B[Leitura automática<br/>+ avisos]
    B --> C{Conferência<br/>humana}
    C -->|corrige / completa| D[Empenho cadastrado<br/>valor total = saldo inicial]
    M[✍️ Cadastro manual] --> D
    D --> E[Nota Fiscal / NFS-e<br/>vinculada ao empenho]
    E -->|valor ≤ saldo| F[Central de vencimentos<br/>vencimento + dias de atraso]
    E -.->|valor > saldo| X[❌ Recusada]
    F --> G[Minuta do ofício<br/>principal + multa + juros]
```

Na tela, o fluxo é dividido em cinco blocos:

1. **Analisar empenhos em PDF** — envia até 10 PDFs por vez.
2. **Conferir e cadastrar empenho** — revisa os campos extraídos (ou digita tudo manualmente) e salva.
3. **Vincular Nota Fiscal / NFS-e** — escolhe o empenho e lança a nota.
4. **Empenhos cadastrados** — lista paginada.
5. **Central de vencimentos e cobrança** — mostra cada nota pendente com vencimento e atraso; informa multa e juros do contrato e baixa a minuta.

---

## 📋 Regras de negócio

Esta seção descreve **exatamente o que o sistema faz hoje**, conforme o código. Cada regra é validada no servidor — não dá para contorná-la pelo navegador.

### 1. Empresa e acesso

| Regra | Detalhe |
|---|---|
| Uma empresa por instalação | Cada servidor atende **uma única empresa** (razão social + CNPJ), definida na configuração. O navegador não consegue escolher outra. |
| CNPJ da empresa | Precisa ter 14 dígitos. Pontuação (`.`, `/`, `-`) é ignorada. Não há consulta à Receita nem validação dos dígitos verificadores. |
| Empenho só da própria empresa | O CNPJ do favorecido no empenho precisa ser **igual** ao CNPJ da empresa. Caso contrário, o cadastro é recusado ("O registro não pertence à empresa configurada"). |
| Modo demonstração | Sem token configurado, o sistema só funciona no próprio computador (`localhost`). |
| Acesso pela internet | Exige um token de acesso de 32 a 256 caracteres. Sem token, o servidor **se recusa a iniciar** em endereço externo. |
| Usuários e login | **Ainda não existem.** Quem tem o token acessa tudo da empresa configurada. |

### 2. Leitura do PDF do empenho

O leitor foi validado com os **7 PDFs reais do SIAFI** em [`docs/empenho/`](docs/empenho/).

**Campos extraídos:**

| Campo | De onde vem no PDF | Observação |
|---|---|---|
| UG (Unidade Gestora) | Bloco "UG Emitente" | 6 dígitos |
| CNPJ do emitente | Bloco "UG Emitente" | Órgão que emitiu |
| Nº do empenho | Linha "ANO NE NÚMERO" | Montado no formato `2026NE000290` |
| CNPJ e nome do fornecedor | Bloco "Favorecido" | É a empresa que vai receber |
| Valor total | Linha da data de emissão | Valor do cabeçalho, em reais |
| Data de emissão | Bloco "Data de Emissão" | |
| Modalidade | Bloco "Amparo Legal" | Pregão, Dispensa, Inexigibilidade, Concorrência |
| Nº do edital / ata | Bloco "Descrição" ou, se ausente, "Informação Complementar" (código UASG) | Formato `90012/2025` |
| Gestão | Linha "Gestão: NNNNN" | Os PDFs atuais **não trazem** Gestão |

**Classificação da origem da contratação:**

| Origem | Quando é atribuída |
|---|---|
| `licitacao` | Modalidade Pregão (inclusive **SRP**) ou Concorrência |
| `contratacao_direta` | Dispensa ou Inexigibilidade |
| `adesao_ata` (carona) | **Somente** quando a descrição cita explicitamente "adesão à ata", "carona" ou "não participante" |
| `nao_identificada` | Nenhuma das anteriores |

> **Pregão SRP não é carona.** Um registro de preços é licitação. A carona só é sugerida com menção explícita no documento.
>
> **UGR não é Gestão.** O sistema não usa a UGR para preencher a Gestão.

**Avisos e erros:**

- Cada campo obrigatório não encontrado (UG, Gestão, Empenho, CNPJ do favorecido, Valor, Edital/ata) gera um aviso "confira manualmente".
- PDF que não segue o layout SIAFI gera o aviso "Layout não reconhecido" — todos os campos devem ser conferidos.
- **O nome do arquivo nunca é usado** para deduzir dados.
- PDF escaneado (sem texto), protegido por senha, corrompido ou acima dos limites retorna **erro explícito**. Não há OCR.
- Os PDFs são processados em arquivo temporário e **apagados logo depois**. O sistema não guarda os anexos.

### 3. Cadastro do empenho

O empenho pode vir da leitura do PDF ou ser digitado à mão. Em ambos os casos:

| Campo | Regra |
|---|---|
| UG | Obrigatória, exatamente 6 dígitos |
| Gestão | Opcional; se preenchida, exatamente 5 dígitos |
| Nº do empenho | Obrigatório, até 64 caracteres |
| CNPJ do fornecedor | 14 dígitos e **igual ao CNPJ da empresa** |
| Modalidade | Até 200 caracteres |
| Nº edital / ata | Até 100 caracteres |
| Valor total | Maior que zero e até R$ 10 bilhões; digitado no formato `1.500,00` (centavos opcionais) |

**Duplicidade:** o mesmo número de empenho **não pode ser cadastrado duas vezes na mesma UG** para a mesma empresa ("Empenho já cadastrado para esta UG"). O mesmo número em UGs diferentes é permitido.

### 4. Notas fiscais e saldo

Toda nota é vinculada a um empenho.

| Campo | Regra |
|---|---|
| Tipo | `Nota Fiscal (Produtos)` ou `NFS-e (Serviços)` |
| Número | Obrigatório, até 100 caracteres |
| Valor | Maior que zero |
| Data de emissão | Data válida |
| Prazo de pagamento | De 1 a 3.650 dias (prazo do edital/contrato) |

**Regra do saldo:**

```
saldo do empenho = valor total do empenho − soma de todas as notas já vinculadas
```

- Uma nota só é aceita se **valor da nota ≤ saldo**. Caso contrário: "Valor da nota excede o saldo do empenho".
- A regra vale **mesmo com duas pessoas lançando notas ao mesmo tempo**: o cálculo do saldo e a gravação acontecem numa operação única e travada no banco.
- **Duplicidade:** a mesma combinação número + tipo não pode ser lançada duas vezes no mesmo empenho ("Nota já vinculada ao empenho").
- Toda nota nasce com situação **PENDENTE** e aparece na central de vencimentos.

### 5. Vencimento, atraso e encargos

**Vencimento:**

```
vencimento = data de emissão da NF + prazo de pagamento (dias corridos)
```

**Dias de atraso:** diferença entre hoje e o vencimento, nunca negativa. "Hoje" é sempre o dia em **horário de Brasília (UTC−3)**.

**Encargos** (informados pelo usuário conforme o contrato; o padrão é zero):

| Encargo | Fórmula | Quando incide |
|---|---|---|
| Multa de mora | `principal × multa%` | Uma única vez, a partir do 1º dia de atraso |
| Juros de mora | `principal × juros% ao mês × dias de atraso ÷ 30` | Proporcional ao dia (*pro rata die*), juros simples |
| Total | `principal + multa + juros` | |

- Multa e juros aceitam de 0% a 1.000%, com duas casas (ex.: `2,00%`).
- Todos os valores são calculados **em centavos inteiros** e arredondados para o centavo mais próximo — sem erro de arredondamento de ponto flutuante.
- Nota sem atraso: multa e juros são zero.

**Exemplo** (o mesmo usado nos testes automatizados):

| Item | Valor |
|---|---|
| NF de R$ 100,00 emitida em 01/09/2026, prazo de 30 dias | vence em **01/10/2026** |
| Consulta em 16/10/2026 | **15 dias** de atraso |
| Multa de 2% | R$ 2,00 |
| Juros de 1% a.m. → 100 × 1% × 15 ÷ 30 | R$ 0,50 |
| **Total a cobrar** | **R$ 102,50** |

> A central de vencimentos mostra apenas principal, vencimento e atraso. Multa e juros entram no cálculo da minuta, com as taxas que o usuário informar.

### 6. Minuta do ofício de cobrança

Para cada nota pendente, o sistema gera um arquivo `.txt` (`Oficio_Cobranca_<id>.txt`) com:

- destinatário: a Unidade Gestora (UG) do empenho;
- referência: nº do empenho, modalidade, edital/ata e nº da NF;
- data de emissão, valor principal, vencimento e dias de atraso;
- multa, juros e total calculados com as taxas informadas;
- fundamentação: **art. 92, V** (condições de pagamento e atualização monetária) e **art. 141** (ordem cronológica de pagamentos) da [Lei nº 14.133/2021](https://www.planalto.gov.br/ccivil_03/_ato2019-2022/2021/lei/l14133.htm);
- assinatura com razão social e CNPJ da empresa.

> É uma **minuta** para revisão. O texto pede esclarecimentos e regularização; ele **não afirma que houve infração** só porque há atraso. Os encargos dependem do que o contrato prevê e da conferência da data inicial.

### 7. O que o sistema não faz

Para evitar expectativas erradas, **hoje o Felix não**:

- ❌ envia a cobrança automaticamente (o ofício é baixado e enviado pelo usuário);
- ❌ registra pagamento ou baixa de nota (toda nota permanece PENDENTE);
- ❌ lê notas fiscais em PDF (são lançadas à mão; só o empenho é lido automaticamente);
- ❌ faz OCR de documentos escaneados;
- ❌ guarda os PDFs enviados;
- ❌ consulta Receita Federal, CGU ou Portal da Transparência;
- ❌ tem login, usuários, papéis ou várias empresas na mesma instalação.

Vários desses itens estão no [Roadmap](#-roadmap).

---

## 📖 Glossário

| Termo | Significado |
|---|---|
| **Empenho / NE** | Nota de Empenho: documento em que o órgão reserva o dinheiro e se compromete a pagar o fornecedor |
| **SIAFI** | Sistema federal que emite as Notas de Empenho |
| **UG** | Unidade Gestora: o órgão (ou setor) que emitiu o empenho e vai pagar |
| **Gestão** | Código de 5 dígitos que complementa a UG |
| **UGR** | Unidade Gestora Responsável — não confundir com Gestão |
| **NF / NFS-e** | Nota fiscal de produtos / nota fiscal de serviços eletrônica |
| **Saldo do empenho** | Quanto ainda pode ser faturado contra aquele empenho |
| **SRP** | Sistema de Registro de Preços (uma forma de pregão) |
| **Carona / adesão à ata** | Órgão que usa a ata de outro órgão sem ter participado da licitação |
| **Pro rata die** | Juros proporcionais aos dias de atraso |

---

## 🚀 Início rápido

**Requisitos:** Rust 1.96 (fixado em `rust-toolchain.toml`), Poppler (`pdfinfo`, `pdftotext`) e util-linux (`prlimit`).

```bash
# Debian/Ubuntu
sudo apt install poppler-utils util-linux

make build
./target/release/felix-server
```

Abra <http://127.0.0.1:8080>. O padrão é modo demonstração, com CNPJ fictício e banco `saas_cobrancas.db` na pasta atual. `make run` executa a versão de desenvolvimento lendo o `.env`.

**Analisar PDFs pela linha de comando** (sem abrir o servidor e sem gravar no banco):

```bash
./target/release/felix-server analisar docs/empenho/*.pdf
```

Cada linha de saída é um JSON com os dados extraídos e os avisos.

---

## ⚙️ Configuração

Copie `.env.example` para `.env` e ajuste. O binário lê variáveis do processo e **não carrega o `.env` sozinho**:

```bash
set -a; source .env; set +a
./target/release/felix-server
```

| Variável | Padrão | Finalidade |
|---|---|---|
| `FELIX_BIND` | `127.0.0.1:8080` | Endereço e porta. Endereço externo exige token |
| `DATABASE_PATH` | `saas_cobrancas.db` | Arquivo SQLite (compatível com o banco da versão anterior) |
| `FELIX_EMPRESA_NOME` | `Demonstração local` | Razão social da empresa atendida |
| `FELIX_EMPRESA_CNPJ` | `12345678000190` | CNPJ com 14 dígitos |
| `FELIX_API_TOKEN` | *(vazio)* | Token Bearer de 32–256 caracteres ASCII. Obrigatório fora do `localhost` |
| `RUST_LOG` | `felix_server=info` | Logs em JSON, sem texto dos documentos nem token |

> Nunca publique o `.env`. Ele já está no `.gitignore`.

---

## 🔌 API

Todas as rotas `/api/*` exigem `Authorization: Bearer <token>` quando há token configurado. Valores monetários trafegam em **centavos inteiros**; taxas em **centésimos de ponto percentual** (`100` = 1%).

| Método | Rota | Descrição |
|---|---|---|
| `GET` | `/api/empresa` | Empresa configurada, modo e limites |
| `POST` | `/api/analisar` | Envia PDFs (multipart) e devolve dados extraídos + avisos |
| `GET` | `/api/empenhos?offset=0` | Lista empenhos (100 por página) |
| `POST` | `/api/empenhos` | Cadastra empenho |
| `POST` | `/api/empenhos/{id}/notas` | Vincula NF ao empenho (valida saldo) |
| `GET` | `/api/notas?offset=0` | Notas pendentes com vencimento e atraso |
| `GET` | `/api/notas/{id}/oficio?multa_bps=200&juros_bps=100` | Baixa a minuta do ofício |
| `GET` | `/health` | Saúde e PDFs em processamento |

**Códigos de resposta:**

| Código | Significado |
|---|---|
| `400` | Dado inválido (formato, faixa, campo obrigatório) |
| `401` | Token ausente ou inválido |
| `403` | Registro de outra empresa, origem não autorizada ou demonstração fora do `localhost` |
| `409` | Empenho/nota duplicado ou nota acima do saldo |
| `413` | Arquivo ou lote acima do limite |
| `429` | Servidor ocupado — tente de novo em 2 s (`Retry-After: 2`) |

---

## 📏 Limites operacionais

Dimensionado para uma VPS de **2 CPUs / 4 GiB**.

| Recurso | Limite |
|---|---|
| Arquivos | 5 MiB por PDF; até 10 arquivos e 20 MiB por lote |
| Documento | Até 100 páginas e 2 MiB de texto (recusa em vez de cortar) |
| PDFs processados em paralelo | 2 por processo |
| Análises admitidas ao mesmo tempo | 8; o excedente recebe `429` |
| Subprocesso Poppler | 384 MiB de memória, 8 s de CPU, 10 s por comando |
| Listagens | 100 registros por página |
| Requisição | Tempo máximo de 60 s |

Medição local documentada em [`docs/validacao-rust.md`](docs/validacao-rust.md). A VPS real ainda precisa ser validada.

---

## 🏗️ Arquitetura

```text
crates/
├── felix-core/         # Regras puras: dinheiro, cálculo de cobrança, parser SIAFI
└── felix-server/       # Configuração, SQLite, subprocessos PDF, HTTP e CLI
    ├── examples/       # Teste de carga
    └── tests/          # PDFs reais, API, limites, banco legado e concorrência
web/                    # HTML/CSS/JS sem framework, embutido no binário
tests/                  # Teste do frontend (Node)
deploy/                 # Modelos de systemd e nginx (não instalados)
docs/                   # PDFs de referência, validação e planejamento SaaS
```

- **Um único binário**: a interface web vai embutida; não precisa de Node nem build de frontend para rodar.
- **Dinheiro sem ponto flutuante**: domínio e API usam centavos (`i64`). Colunas antigas `REAL` são lidas com arredondamento explícito para centavos.
- **Banco legado preservado**: tabelas e colunas do SQLite anterior são mantidas; só são criados índices. WAL ativo. **Faça backup antes de abrir um banco existente.**
- **Segurança**: `unsafe` proibido; `unwrap`/`expect`/`panic` negados em produção; comparação de token em tempo constante; CSP restritiva; PDFs processados sem shell e com limites de recursos.

---

## 🛠️ Desenvolvimento

```bash
make test       # testes Rust + frontend
make lint       # fmt, clippy -D warnings, checagem do JS
make coverage   # cobertura mínima de 80% (requer cargo-llvm-cov)
```

Os testes com PDFs reais **falham** (não são pulados) se os arquivos ou o Poppler estiverem ausentes. Node é necessário só para o teste do frontend.

**Teste de carga** com um servidor já iniciado:

```bash
cargo build --release --locked --example carga -j 2
./target/release/examples/carga 8080 140 16 PID_DO_SERVIDOR
```

---

## 📦 Deploy

**Docker** (usuário sem privilégios, dependências de PDF incluídas):

```bash
docker build -t felix-crawler:local .
docker run --rm --cpus=2 --memory=2g --pids-limit=64 --read-only \
  --tmpfs /tmp:rw,noexec,nosuid,size=256m \
  --env-file .env -e FELIX_BIND=0.0.0.0:8080 \
  -e DATABASE_PATH=/data/saas_cobrancas.db \
  -p 127.0.0.1:8080:8080 -v felix-data:/data felix-crawler:local
```

**systemd + nginx:** [`deploy/felix.service`](deploy/felix.service) limita a aplicação a 180% de CPU e 2 GiB de RAM; [`deploy/nginx.conf.example`](deploy/nginx.conf.example) adiciona limites de conexão e cabeçalhos. São modelos para revisar. Configure **TLS e token** antes de expor à internet.

---

## 🗺️ Roadmap

A próxima fase transforma o Felix em **SaaS multiempresa**. Planejado, **ainda não implementado**:

- [ ] Cadastro, login e papéis (dono, operador, leitor, admin)
- [ ] Várias empresas (CNPJs) por organização
- [ ] Ciclo da nota: emitida → liquidada → paga / cancelada
- [ ] Painel: a receber, vencido, aging (0–30 / 31–60 / 61–90 / 90+) e ranking por órgão
- [ ] Alertas de nota vencendo em até 5 dias e de nota vencida
- [ ] Registro de cobranças enviadas (canal, protocolo, data) e histórico
- [ ] Integração com o Portal da Transparência por CNPJ
- [ ] Planos Gratuito / Profissional / Empresarial
- [ ] Exportação CSV e trilha de auditoria

Detalhes em [`docs/saas/`](docs/saas/README.md).

---

## 📚 Documentação

| Documento | Conteúdo |
|---|---|
| [`docs/saas/01-produto.md`](docs/saas/01-produto.md) | Personas, fluxos, escopo do MVP SaaS e planos |
| [`docs/saas/00-pesquisa.md`](docs/saas/00-pesquisa.md) | Mercado, Lei 14.133/2021, IN 77/2022 e dados públicos |
| [`docs/saas/02-arquitetura.md`](docs/saas/02-arquitetura.md) | Arquitetura e modelo de dados do SaaS |
| [`docs/saas/03-api-v1.md`](docs/saas/03-api-v1.md) | Contrato da API v1 |
| [`docs/saas/decisoes.md`](docs/saas/decisoes.md) | Registro de decisões técnicas |
| [`docs/validacao-rust.md`](docs/validacao-rust.md) | Medições de desempenho |
| [`docs/planos/`](docs/planos/) | Planos de migração |
