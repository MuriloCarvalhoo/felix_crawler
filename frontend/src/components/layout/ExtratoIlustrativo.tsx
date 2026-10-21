import { formatarBRL } from '@/lib/formatos'

/*
 * Painel lateral das telas públicas: um extrato de recebíveis fictício que mostra,
 * em vez de descrever, o que o produto faz: total a receber, idade da dívida
 * por faixa e notas com sua situação.
 */

const FAIXAS = [
  { rotulo: 'A vencer', parte: 51, cor: 'bg-[#4fb89b]' },
  { rotulo: '1–30 dias', parte: 26, cor: 'bg-[#e6b45c]' },
  { rotulo: '31–60', parte: 12, cor: 'bg-[#e58f55]' },
  { rotulo: '61–90', parte: 7, cor: 'bg-[#f08072]' },
  { rotulo: '90+', parte: 4, cor: 'bg-[#c4473a]' },
]

const NOTAS = [
  { nf: 'NF 2.311', orgao: 'Hospital Universitário, UG 155007', centavos: 4_892_000, situacao: 'Vencida há 12 dias', cor: 'text-[#f59a8e]' },
  { nf: 'NF 2.305', orgao: 'Instituto Federal, UG 158366', centavos: 9_630_050, situacao: 'Liquidada', cor: 'text-[#93b8e4]' },
  { nf: 'NF 2.298', orgao: 'Base Aérea, UG 120071', centavos: 1_264_000, situacao: 'Vence em 3 dias', cor: 'text-[#e6b45c]' },
  { nf: 'NF 2.287', orgao: 'Superintendência Regional, UG 200354', centavos: 3_100_000, situacao: 'Paga em 14/10', cor: 'text-[#6fcdb2]' },
]

export function ExtratoIlustrativo() {
  return (
    <aside
      aria-label="Exemplo do painel de recebíveis"
      className="relative hidden min-w-0 flex-col justify-center gap-12 overflow-hidden bg-[#13261f] px-12 py-12 text-[#d3e0da] lg:flex xl:px-16"
    >
      <div className="max-w-[30rem]">
        <h2 className="text-[2.125rem] leading-[1.12] font-semibold tracking-[-0.025em] text-white">
          Quanto o governo deve à sua empresa, e desde quando.
        </h2>
        <p className="mt-4 max-w-[46ch] text-corpo text-[#a9bdb4]">
          Empenhos, notas fiscais e pagamentos num só lugar. Alerta antes do vencimento e cobrança com multa e juros
          já calculados.
        </p>
      </div>

      <figure className="max-w-[32rem] rounded-janela border border-white/10 bg-[#1c3329] p-6 shadow-[0_24px_60px_rgb(0_0_0/0.35)]">
        <div className="flex items-end justify-between gap-4">
          <div>
            <p className="text-detalhe text-[#92a89e]">A receber</p>
            <p className="cifras mt-1 text-[2rem] leading-none font-semibold tracking-[-0.02em] text-white">
              {formatarBRL(15_786_050)}
            </p>
          </div>
          <p className="cifras text-right text-detalhe text-[#92a89e]">
            3 notas em aberto
            <br />
            em 3 órgãos
          </p>
        </div>

        <div className="mt-6" aria-hidden="true">
          <div className="flex h-2.5 overflow-hidden rounded-full bg-white/5">
            {FAIXAS.map((f) => (
              <span key={f.rotulo} className={`${f.cor} h-full border-r-2 border-[#1c3329] last:border-r-0`} style={{ width: `${String(f.parte)}%` }} />
            ))}
          </div>
          <div className="mt-2.5 flex flex-wrap gap-x-4 gap-y-1 text-mini text-[#92a89e]">
            {FAIXAS.map((f) => (
              <span key={f.rotulo} className="inline-flex items-center gap-1.5">
                <span className={`size-2 rounded-[2px] ${f.cor}`} />
                {f.rotulo}
              </span>
            ))}
          </div>
        </div>

        <ul className="mt-6 divide-y divide-white/8 border-t border-white/8">
          {NOTAS.map((n) => (
            <li key={n.nf} className="flex items-center justify-between gap-4 py-3">
              <div className="min-w-0">
                <p className="text-detalhe font-semibold text-white">{n.nf}</p>
                <p className="truncate text-mini text-[#92a89e]">{n.orgao}</p>
              </div>
              <div className="shrink-0 text-right">
                <p className="cifras text-detalhe text-white">{formatarBRL(n.centavos)}</p>
                <p className={`text-mini ${n.cor}`}>{n.situacao}</p>
              </div>
            </li>
          ))}
        </ul>
        <figcaption className="mt-4 text-mini text-[#6f857b]">Exemplo ilustrativo com dados fictícios.</figcaption>
      </figure>
    </aside>
  )
}
