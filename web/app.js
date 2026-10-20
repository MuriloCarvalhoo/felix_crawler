"use strict";
const $ = id => document.getElementById(id);
let token = "", empOffset = 0, nfOffset = 0;
const moeda = centavos => (centavos / 100).toLocaleString("pt-BR", {style:"currency",currency:"BRL"});
function centavos(texto) {
  const valor = texto.trim();
  if (!/^(?:\d+|\d{1,3}(?:\.\d{3})+)(?:,\d{2})?$/.test(valor)) throw Error("Use valores como 1.500,00.");
  const [reais, cents="00"] = valor.replaceAll(".", "").split(",");
  const total = Number(reais) * 100 + Number(cents);
  if (!Number.isSafeInteger(total) || total <= 0 || total > 1e12) throw Error("Valor fora dos limites.");
  return total;
}
function mensagem(texto, erro=false) { $("mensagem").textContent=texto; $("mensagem").className=erro?"erro":"sucesso"; }
async function api(path, options={}) {
  const response=await fetch("/api"+path,{...options,headers:{...(token?{Authorization:"Bearer "+token}:{}),...options.headers}});
  if (!response.ok) {
    const body=await response.json().catch(()=>({erro:"Falha HTTP "+response.status}));
    throw Error(body.erro || "Falha HTTP "+response.status);
  }
  return response;
}
async function json(path,options) { return (await api(path,options)).json(); }
function linha(parent,values) {
  const tr=document.createElement("tr");
  values.forEach(value=>{const td=document.createElement("td");if(value instanceof Node)td.append(value);else td.textContent=value;tr.append(td);});parent.append(tr);
}
function paginar(prefix,page) {
  $(prefix+"-pagina").textContent=`${page.total ? page.offset+1 : 0}–${Math.min(page.offset+page.limite,page.total)} de ${page.total}`;
  $(prefix+"-anterior").disabled=page.offset===0;$(prefix+"-proximo").disabled=page.offset+page.limite>=page.total;
}
async function atualizar() {
  // Consultas sequenciais evitam competir com a própria admissão do banco.
  const emp=await json("/empenhos?offset="+empOffset);
  const escolha=$("empenho-select").value, escolhaTexto=$("empenho-select").selectedOptions[0]?.textContent;
  $("empenhos").replaceChildren();$("empenho-select").replaceChildren();
  emp.itens.forEach(item=>{linha($("empenhos"),[item.empenho,item.ug,item.modalidade,moeda(item.valor_total_centavos)]);const option=document.createElement("option");option.value=item.id;option.textContent=`${item.empenho} / UG ${item.ug} — ${moeda(item.valor_total_centavos)}`;$("empenho-select").append(option);});
  if(escolha){if(!Array.from($("empenho-select").options).some(option=>String(option.value)===escolha)){const option=document.createElement("option");option.value=escolha;option.textContent=escolhaTexto;$("empenho-select").append(option);}$("empenho-select").value=escolha;}
  paginar("emp",emp);
  const notas=await json("/notas?offset="+nfOffset);$("notas").replaceChildren();
  notas.itens.forEach(({nota,calculo})=>{
    const button=document.createElement("button");button.textContent="Baixar ofício";button.type="button";
    button.disabled=!calculo || calculo.dias_atraso===0;
    button.addEventListener("click",async()=>{try{
      const multa=Math.round(Number($("multa").value)*100),juros=Math.round(Number($("juros").value)*100);
      if(!Number.isInteger(multa)||!Number.isInteger(juros)||multa<0||juros<0||multa>100000||juros>100000)throw Error("Confira as taxas.");
      const response=await api(`/notas/${nota.nf_id}/oficio?multa_bps=${multa}&juros_bps=${juros}`);
      const url=URL.createObjectURL(await response.blob());const a=document.createElement("a");a.href=url;a.download=`Oficio_Cobranca_${nota.nf_id}.txt`;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
    }catch(error){mensagem(error.message,true);}});
    linha($("notas"),[`${nota.numero_nf} / ${nota.empenho}`,moeda(nota.valor_nf_centavos),calculo?calculo.vencimento.split("-").reverse().join("/"):"Revisar data",calculo?`${calculo.dias_atraso} dia(s)`:"Revisar",button]);
  });paginar("nf",notas);
}
async function conectar(){const info=await json("/empresa");$("empresa").textContent=`${info.empresa.razao_social} · CNPJ ${info.empresa.cnpj} · ${info.modo==="demonstracao"?"Demonstração local":"Acesso autenticado"}`;$("acesso").hidden=info.modo==="demonstracao";await atualizar();}
$("acesso-form").addEventListener("submit",async e=>{e.preventDefault();token=$("token").value;try{await conectar();mensagem("Conectado.");}catch(error){mensagem(error.message,true);}});
$("empenho-form").addEventListener("submit",async e=>{e.preventDefault();const button=e.submitter;button.disabled=true;try{
  const values=Object.fromEntries(new FormData(e.target));const {valor,...rest}=values;rest.gestao=rest.gestao||null;
  await json("/empenhos",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({...rest,valor_total_centavos:centavos(valor)})});empOffset=0;await atualizar();mensagem("Empenho cadastrado.");
}catch(error){mensagem(error.message,true);}finally{button.disabled=false;}});
$("nota-form").addEventListener("submit",async e=>{e.preventDefault();const button=e.submitter;button.disabled=true;try{
  const values=Object.fromEntries(new FormData(e.target));const {empenho_id,valor,...rest}=values;
  await json(`/empenhos/${empenho_id}/notas`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({...rest,valor_nf_centavos:centavos(valor),prazo_dias_pagamento:Number(rest.prazo_dias_pagamento)})});nfOffset=0;await atualizar();mensagem("Nota Fiscal vinculada ao empenho.");
}catch(error){mensagem(error.message,true);}finally{button.disabled=false;}});
function resultado(nome,result){const box=document.createElement("div");box.className="resultado";const title=document.createElement("strong");title.textContent=nome;box.append(title);
  const detail=document.createElement("p");if(result.erro){detail.textContent=result.erro;box.append(detail);$("resultados").append(box);return;}
  const a=result.analise;detail.textContent=`${a.dados.empenho||"Empenho não identificado"} · ${a.dados.valor_total_centavos?moeda(a.dados.valor_total_centavos):"Valor não identificado"} · ${a.dados.modalidade||"Modalidade não identificada"}`;box.append(detail);
  a.avisos.forEach(aviso=>{const p=document.createElement("p");p.textContent=aviso;box.append(p);});
  const button=document.createElement("button");button.type="button";button.textContent="Conferir este empenho";button.addEventListener("click",()=>{const form=$("empenho-form");for(const field of ["ug","gestao","empenho","cnpj_fornecedor","numero_edital_ata"])form.elements.namedItem(field).value=a.dados[field]||"";form.elements.namedItem("modalidade").value=(a.dados.modalidade||"")+(a.dados.origem==="adesao_ata"?" — Adesão à ata (carona)":"");form.elements.namedItem("valor").value=a.dados.valor_total_centavos?(a.dados.valor_total_centavos/100).toLocaleString("pt-BR",{minimumFractionDigits:2,maximumFractionDigits:2}):"";form.scrollIntoView({behavior:"smooth",block:"center"});form.elements.namedItem("ug").focus({preventScroll:true});});box.append(button);$("resultados").append(box);
}
$("upload-form").addEventListener("submit",async e=>{e.preventDefault();const files=Array.from($("arquivos").files);if(files.length>10)return mensagem("Selecione até 10 arquivos por vez.",true);if(files.some(file=>file.size>5*1024*1024))return mensagem("Limite de 5 MiB por arquivo.",true);
  $("analisar").disabled=true;$("resultados").replaceChildren();let index=0,done=0;$("progresso").textContent=`0 de ${files.length} processados`;
  async function worker(){while(index<files.length){const file=files[index++];try{let response;for(let tentativa=0;tentativa<4;tentativa++){const data=new FormData();data.append("files",file);response=await fetch("/api/analisar",{method:"POST",body:data,headers:token?{Authorization:"Bearer "+token}:{}});if(response.status!==429)break;await new Promise(resolve=>setTimeout(resolve,2000));}const data=await response.json();if(!response.ok)throw Error(data.erro||"Upload não concluído.");data.resultados.forEach(item=>resultado(item.arquivo,item));}catch(error){resultado(file.name,{erro:error.message});}done++;$("progresso").textContent=`${done} de ${files.length} processados`;}}
  try{await Promise.all(Array.from({length:Math.min(4,files.length)},worker));}finally{$("analisar").disabled=false;}
});
for(const [id,delta,kind] of [["emp-anterior",-100,"emp"],["emp-proximo",100,"emp"],["nf-anterior",-100,"nf"],["nf-proximo",100,"nf"]])$(id).addEventListener("click",async()=>{if(kind==="emp")empOffset=Math.max(0,empOffset+delta);else nfOffset=Math.max(0,nfOffset+delta);try{await atualizar();}catch(error){mensagem(error.message,true);}});
$("emissao").value=new Date().toLocaleDateString("en-CA",{timeZone:"America/Sao_Paulo"});
conectar().catch(error=>mensagem(error.message,true));
