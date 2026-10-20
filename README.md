# Felix — empenhos e cobranças em Rust

Aplicação Linux para extrair dados de notas de empenho, conferir e cadastrar empenhos, vincular NFs/NFS-e, controlar saldo/vencimento e baixar uma minuta de cobrança. **Sem Python, Streamlit, Gemini ou serviço de IA no runtime.** A interface HTML/JS é embutida no binário; não precisa de Node para executar nem de build de frontend.

## Executar localmente

Requisitos: Rust 1.96 (fixado em `rust-toolchain.toml`), Poppler (`pdfinfo` e `pdftotext`) e util-linux (`prlimit`). Em Debian/Ubuntu, os pacotes de runtime são `poppler-utils util-linux`.

```bash
make build
./target/release/felix-server
```

Abra <http://127.0.0.1:8080>. `make run` executa a versão de desenvolvimento. O padrão é demonstração local, CNPJ fictício e banco `saas_cobrancas.db` no diretório atual.

Para analisar arquivos sem abrir o servidor nem gravar no banco:

```bash
./target/release/felix-server analisar docs/empenho/*.pdf
```

Cada linha de saída é um JSON com dados extraídos e avisos. O nome do arquivo não é utilizado para inferir dados do documento.

## Configuração

Copie `.env.example` para `.env`, configure sua empresa e, para acesso via VPS/proxy, gere um token aleatório com pelo menos 32 caracteres. Não publique esse arquivo. O programa lê variáveis do processo; **não carrega `.env` automaticamente**. Em Bash, para um arquivo local confiável:

```bash
set -a
source .env
set +a
./target/release/felix-server
```

| Variável | Padrão / finalidade |
|---|---|
| `FELIX_BIND` | `127.0.0.1:8080`; bind externo exige token |
| `DATABASE_PATH` | `saas_cobrancas.db`; pode apontar para o SQLite anterior |
| `FELIX_EMPRESA_NOME`, `FELIX_EMPRESA_CNPJ` | Empresa atendida por esta instância; CNPJ com 14 dígitos |
| `FELIX_API_TOKEN` | Bearer de 32–256 caracteres ASCII; obrigatório ao expor a aplicação por proxy/VPS |
| `RUST_LOG` | `felix_server=info`; logs JSON, sem texto dos documentos nem token |

A empresa é escolhida no servidor. O navegador não pode enviar outro `empresa_id`. O token dá acesso à empresa configurada; esta versão **não implementa cadastro/login de usuários nem uma carteira de múltiplas empresas com tokens individuais**. Sem token, a API admite apenas hosts locais. A verificação de CNPJ exige igualdade integral e formato numérico, sem consulta cadastral nem validação de dígitos verificadores.

## Extração de PDFs

O parser foi validado nos **7 PDFs SIAFI de `docs/empenho/`**, todos com texto extraível. Ele separa emitente/favorecido, usa o valor do cabeçalho e reconstrói o número por ano/tipo/número. Extrai UG, empenho, CNPJ/nome do fornecedor, CNPJ emitente, valor, emissão, modalidade e número do edital/ata.

- Os arquivos fornecidos não trazem Gestão; ela permanece ausente. **UGR não é Gestão.** O cadastro permite deixar esse campo em branco.
- Pregão/SRP é licitação; carona só é sugerida quando há referência explícita a adesão/não participante no contexto analisado. Campos e classificação exigem conferência humana.
- Layout desconhecido ou campo ausente gera aviso. Não há preenchimento inventado nem fallback para IA.
- Digitalizações sem texto, PDFs protegidos, corrompidos ou fora dos limites retornam erro explícito. **OCR não está habilitado.** A análise automática atual é de empenhos; as notas fiscais são cadastradas manualmente.
- Os PDFs ficam em arquivos temporários privados e são apagados ao fim do processamento. Não há arquivo permanente de anexos.

## Limites para a VPS de 2 CPUs / 4 GiB

| Recurso | Limite implementado |
|---|---|
| PDFs em processamento | 2 globalmente por processo; parser fora dos workers de rede |
| Requisições de análise admitidas | 8, incluindo as em execução; excesso HTTP 429 + `Retry-After: 2` |
| Arquivo / lote | 5 MiB por PDF; 20 MiB e até 10 arquivos por requisição |
| Documento / texto | Até 100 páginas e 2 MiB de texto; recusa em vez de truncamento |
| Subprocesso Poppler | 384 MiB de espaço virtual, 8 s de CPU, 10 s de duração por comando, 64 descritores e 2 MiB de saída |

Cada PDF executa `pdfinfo` e `pdftotext` sequencialmente, sem shell. Upload é escrito em chunks no disco. As vagas ficam com o worker até ele terminar, inclusive após desconexão. Há 2 threads de rede, até 4 auxiliares bloqueantes e 2 operações SQLite admitidas. Respostas de listagem têm no máximo 100 registros por página. A interface envia até 4 arquivos em paralelo e faz tentativas limitadas após 429.

Esses limites controlam admissão/recursos; não prometem vazão para qualquer PDF. A medição local está em [docs/validacao-rust.md](docs/validacao-rust.md). A VPS real ainda precisa ser validada.

## Persistência e cobranças

As tabelas e colunas SQLite anteriores são preservadas. São acrescentados apenas índices; WAL permite leituras durante gravações. Novos cadastros validam CNPJ/UG/valores e recusam empenho duplicado na mesma UG/empresa. NFs não podem superar o saldo mesmo com gravações simultâneas: soma e gravação compartilham `BEGIN IMMEDIATE`.

O domínio e a API usam centavos inteiros (`i64`); taxas usam centésimos de ponto percentual (`100` = 1%). As colunas monetárias antigas permanecem `REAL`: a conversão de leitura arredonda explicitamente para centavos, inclusive para dados legados com mais casas. Não há reescrita em massa desses valores. Faça backup antes de abrir um banco existente; a inicialização ativa WAL e cria índices.

O vencimento parte da emissão da NF e juros mensais são divididos por 30, com arredondamento em centavos. A data de acompanhamento usa UTC−3 (America/Sao_Paulo). Taxas começam em zero e dependem do contrato. O ofício é uma minuta, com referências aos arts. 92, V, e 141 da [Lei nº 14.133/2021](https://www.planalto.gov.br/ccivil_03/_ato2019-2022/2021/lei/l14133.htm), sem concluir uma infração apenas pelo atraso. Não há envio automático da cobrança nem consulta à CGU.

## Estrutura e verificações

```text
crates/felix-core/       # dinheiro, cobrança, parser SIAFI
crates/felix-server/     # configuração, SQLite, subprocessos, HTTP, CLI
web/                    # HTML/CSS/JS sem framework, embutido no binário
crates/felix-server/tests/ # PDFs reais, limites, API e banco legado/concorrente
tests/frontend.test.cjs  # regressão da seleção do empenho ao paginar
deploy/                # modelos de systemd e nginx (não instalados)
```

Padrões usados de `llama.rs-MI50`: workspace Rust 2024, `Result`/erros tipados, unsafe proibido, `unwrap`/`expect`/panic negados em produção, regex compiladas uma vez, empréstimos e estruturas limitadas, thin LTO e uma unidade de codegen no release. Referências consultadas: `docs/rust-praticas-da-documentacao.md` e `docs/rust-memoria-e-desempenho.md`. Não se usa `target-cpu=native`, para manter compatibilidade de CPU dentro da mesma arquitetura. O binário nativo ainda depende da ABI/glibc do sistema de build; para a VPS, prefira a imagem Docker ou compile no ambiente de destino.

```bash
make test
make lint
make coverage
```

Node é necessário apenas para os testes do pequeno frontend; `cargo-llvm-cov` é necessário apenas para cobertura. Testes com PDFs reais falham se os arquivos ou ferramentas necessários estiverem ausentes; não fazem skip silencioso.

Carga local reproduzível, sem cadastros, usando os PDFs reais e um servidor já iniciado:

```bash
cargo build --release --locked --example carga -j 2
./target/release/examples/carga 8080 140 16 PID_DO_SERVIDOR
```

O programa mede latência, 429 retentados, health e RSS amostrado do processo/filhos. Restrinja o servidor a duas CPUs com `taskset` para reproduzir a medição documentada.

## Preparar a VPS

`deploy/felix.service` limita a aplicação a 180% de CPU e 2 GiB de RAM, deixando recursos para o sistema/proxy. Usa usuário dedicado, diretório de estado, temporários privados e limites de processos. `deploy/nginx.conf.example` acrescenta limites de conexões, frequência, cabeçalhos e duração. São modelos para revisar; não foram instalados. Configure TLS e token antes de expor a aplicação, inclusive se o bind do Rust continuar em loopback atrás de nginx.

Também há Dockerfile com usuário sem privilégios e dependências de PDF instaladas. O contexto de build exclui documentos reais, banco, `.env`, chaves e `.git`.

```bash
docker build -t felix-crawler:local .
docker run --rm --cpus=2 --memory=2g --pids-limit=64 --read-only \
  --tmpfs /tmp:rw,noexec,nosuid,size=256m \
  --env-file .env -e FELIX_BIND=0.0.0.0:8080 \
  -p 127.0.0.1:8080:8080 -v felix-data:/data felix-crawler:local
```

No container, configure `DATABASE_PATH=/data/saas_cobrancas.db` se o seu `.env` sobrescrever o padrão. O token é obrigatório para o bind externo usado dentro do container. Nenhum deploy ou alteração na VPS faz parte desta migração.
