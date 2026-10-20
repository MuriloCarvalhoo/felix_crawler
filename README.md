# Felix Crawler

Gestão local de empenhos, notas fiscais e minutas de cobrança. Organização Python baseada no `standalone-ia`: Python 3.14, `uv`, aplicação em `src/`, modelos Pydantic, configuração central e `pytest` com cobertura mínima de 80%.

## Executar

```bash
uv sync --frozen
uv run streamlit run app.py --server.address 127.0.0.1
```

Abra <http://127.0.0.1:8501>. `make run` executa o mesmo comando; `main.py` continua disponível como entrada compatível. Sem chave Gemini, os formulários funcionam com preenchimento manual.

Para carregar configurações locais, copie `.env.example` para `.env`, preencha os campos necessários e execute:

```bash
uv run --env-file .env streamlit run app.py --server.address 127.0.0.1
```

Variáveis de ambiente têm prioridade sobre `.streamlit/secrets.toml`. As chaves `CHAVE_IA` e `CHAVE_CGU` mantêm os nomes do original. `GEMINI_MODEL` seleciona o modelo; `DATABASE_PATH` aponta para o SQLite existente. Nenhuma credencial do projeto de referência foi copiada.

## Estrutura

```text
app.py                    # entrada Streamlit
main.py                   # compatibilidade com a entrada antiga
src/
  clients/gemini.py       # API e validação da resposta estruturada
  controller/cobrancas.py # coordenação das operações por empresa
  db/repository.py       # SQLite, transações e filtros por empresa
  models/documents.py    # entradas e registros tipados
  services/              # PDF, vencimento, encargos e minuta
  ui/application.py      # formulários e apresentação
  utils/                 # configuração e erros padronizados
tests/                   # regras, integração SQLite e interface Streamlit
```

O banco usa as mesmas tabelas e colunas do código recebido; a inicialização é idempotente. Os cálculos usam `Decimal`, com juros mensais divididos por 30 e arredondamento de encargos em centavos. As colunas monetárias SQLite continuam `REAL` para manter compatibilidade; uma migração para centavos inteiros não faz parte desta adaptação.

A soma das NFs é verificada dentro de uma transação `BEGIN IMMEDIATE`, impedindo exceder o empenho mesmo com gravações simultâneas. O CNPJ deve ter 14 dígitos e coincidir integralmente com a empresa; isso não valida dígitos verificadores nem consulta cadastro oficial.

## Validação local

```bash
make test
make lint
```

Os testes exercitam cadastro manual, notas vencidas e a vencer, geração da minuta, isolamento entre empresas, saldo concorrente, PDFs e respostas Gemini. A API Gemini é simulada nos testes; nenhuma chamada paga é necessária.

## Limites do código recebido

- **Sessão demonstrativa:** usa a primeira empresa do banco, criando uma empresa fictícia se necessário. Há filtros por empresa, mas ainda não há autenticação nem autorização por usuário. Execute apenas localmente até implementar essa camada.
- **Documentos:** o upload extrai texto e preenche campos; os PDFs não são armazenados. PDFs digitalizados sem camada de texto exigem preenchimento manual; não há OCR.
- **Cobrança:** o prazo parte da emissão da NF, como no original. Confirme o termo inicial e as taxas contratuais antes de usar a minuta. As taxas começam em zero; não há multa ou juros presumidos automaticamente.
- **Integrações:** `CHAVE_CGU` está disponível na configuração, mas o original não implementava consulta à CGU. A conexão real com Gemini depende de chave e acesso ao modelo.

A minuta referencia o art. 92, V, para condições de pagamento e atualização monetária, e solicita esclarecimento sobre a ordem cronológica do art. 141, sem inferir uma infração apenas pelo atraso. Referência: [Lei nº 14.133/2021](https://www.planalto.gov.br/ccivil_03/_ato2019-2022/2021/lei/l14133.htm).

O cliente usa `google-genai` e resposta JSON estruturada conforme a [documentação oficial do SDK](https://googleapis.github.io/python-genai/index.html).
