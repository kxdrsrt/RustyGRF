# RustyGRF – Português (Brasil) (Beta: rascunho automático, ainda não revisado por falantes nativos).

tb-main = Principal
    .aria-label = Principal
tb-app = Aplicativo
    .aria-label = Aplicativo
tb-archive = Arquivo GRF
    .aria-label = Arquivo GRF
tb-edit = Edição
    .aria-label = Edição
tb-edit-mark = Edição
tb-view = Exibição e filtros
    .aria-label = Exibição e filtros

btn-settings = Configurações
    .title = Configurações (Cmd/Ctrl+,)
    .aria-label = Configurações
btn-open = Abrir…
    .title = Abrir um arquivo GRF (Cmd/Ctrl+O)
btn-build = Criar…
    .title = Empacotar uma pasta em um novo GRF
btn-merge = Mesclar…
    .title = Mesclar vários GRFs em um novo
btn-extract = Extrair…
    .title = Extrair as entradas selecionadas, ou todas as exibidas
btn-verify = Verificar
    .title = Verificar se o arquivo tem erros
btn-add = Adicionar arquivo…
    .title = Adicionar um arquivo do disco em um caminho do GRF
btn-replace = Substituir…
    .title = Substituir a entrada selecionada por um arquivo do disco
btn-remove = Remover
    .title = Deixar a entrada selecionada fora do arquivo salvo
btn-undo = Desfazer
    .title = Desfazer a última alteração pendente (Cmd/Ctrl+Z)
btn-redo = Refazer
    .title = Refazer (Shift+Cmd/Ctrl+Z)
btn-save = Salvar…
    .title = Revisar e salvar as alterações pendentes (Cmd/Ctrl+S salva na hora)

search = Buscar
    .placeholder = Buscar: um nome ou um padrão como *.spr
    .aria-label = Buscar

pv-group = Posição da visualização
    .aria-label = Posição da visualização
pv-right = Direita
    .title = Visualização ao lado da lista
    .aria-label = Visualização à direita
pv-bottom = Abaixo
    .title = Visualização abaixo da lista
    .aria-label = Visualização abaixo
pv-off = Não
    .title = Ocultar a visualização (Cmd/Ctrl+Shift+P)
    .aria-label = Ocultar a visualização

view-group = Layout da lista
    .aria-label = Layout da lista
view-tree = Árvore
    .title = Árvore de pastas com arquivos (Cmd/Ctrl+1)
view-folders = Pastas
    .title = Pastas à esquerda, arquivos da pasta escolhida à direita (Cmd/Ctrl+2)
view-list = Lista
    .title = Lista simples: nome, pasta, tamanho (Cmd/Ctrl+3)
view-details = Detalhes
    .title = Tabela simples com colunas ordenáveis (Cmd/Ctrl+4)
view-gallery = Galeria
    .title = Galeria de miniaturas de imagens e sprites (Cmd/Ctrl+5)

sort = Ordem
sort-archive = Ordem salva
sort-name = Nome
sort-path = Caminho
sort-type = Tipo
sort-size = Tamanho
sort-stored = Tamanho compactado
sort-offset = Posição no arquivo
sort-dir = Direção da ordenação
    .title = Crescente ou decrescente
    .aria-label = Direção da ordenação

kind-filter-title = Mostrar apenas alguns tipos de arquivo
kind-label-all = Tipo: todos
kind-label-pictures = Tipo: imagens
kind-label-one = Tipo: { $kind }
kind-label-n = Tipo: { $n } tipos
kind-all = Todos
kind-only-images = Só imagens
kind-image = Imagens
kind-sprite = Sprites
kind-animation = Animações
kind-palette = Paletas
kind-map = Mapas
kind-model = Modelos
kind-audio = Áudio
kind-text = Texto e Lua
kind-effect = Efeitos
kind-file = Outros

size = Tamanho
size-any = qualquer
size-over-1k = mais de 1 KB
size-over-100k = mais de 100 KB
size-over-1m = mais de 1 MB
size-over-10m = mais de 10 MB
size-under-1k = menos de 1 KB
size-under-100k = menos de 100 KB
size-under-1m = menos de 1 MB

subfolders = Subpastas
    .title = Listar também os arquivos das subpastas
expand-all = Expandir tudo
    .title = Abrir todas as pastas
collapse-all = Recolher tudo
    .title = Fechar todas as pastas
reset-filters = Limpar
    .title = Limpar a busca e os filtros de tipo e tamanho

empty-hint = Abra um arquivo GRF ou arraste-o para esta janela.
open-archive = Abrir arquivo…
folders-pane = Pastas
    .aria-label = Pastas
all-files = Todos os arquivos
    .title = Mostrar os arquivos de todas as pastas
col-name = Nome
col-type = Tipo
col-size = Tamanho
col-stored = Compactado
entries-list = Entradas
    .aria-label = Entradas
preview-pane = Visualização
    .aria-label = Visualização
split-folders = Redimensionar a lista de pastas
    .aria-label = Redimensionar a lista de pastas
split-preview = Redimensionar a visualização (setas do teclado)
    .aria-label = Redimensionar a visualização (setas do teclado)
    .title = Arraste, ou foque e use as setas do teclado

status-ready = Pronto
count-entries = { $n ->
    [one] { $n } entrada
   *[other] { $n } entradas
 }
count-filtered = { $n } de { $total }
count-note-folders = (a busca abrange todas as pastas)
count-note-pictures = (imagens e sprites)
names = Nomes
    .title = Como os nomes de arquivo são exibidos e buscados. Ao extrair, sempre são gravados nomes Latin-1, para que o resultado combine com `rgrf build`.
names-korean = Coreano

save-n = { $n ->
    [one] Revisar e salvar a { $n } alteração pendente
   *[other] Revisar e salvar as { $n } alterações pendentes
 }
save-none = Salvar uma cópia com outras configurações
save-aria-n = { $n ->
    [one] Salvar { $n } alteração pendente
   *[other] Salvar { $n } alterações pendentes
 }
save-aria-none = Salvar alterações

dlg-ok = OK
dlg-cancel = Cancelar
dlg-close = Fechar
dlg-done = Concluir
dlg-quick = Rápida
dlg-full = Completa
dlg-edit-animation = Editar animação
dlg-edit-pixels = Editar pixels
dlg-edit-text = Editar texto

verify-title = Verificar
verify-body = A verificação rápida lê só a tabela de arquivos. A completa também decodifica cada entrada, o que lê o arquivo inteiro.
verify-running = Verificando …
verify-status = Verificado: { $errors } erros, { $warnings } avisos
verify-summary-full = { $entries } entradas, todas decodificadas: { $errors } erros, { $warnings } avisos, { $infos } observações.
verify-summary-quick = { $entries } entradas, só a tabela: { $errors } erros, { $warnings } avisos, { $infos } observações.
verify-problems = Problemas encontrados
verify-ok = Arquivo verificado
verify-failed = A verificação falhou

set-title = Configurações
    .aria-label = Configurações
set-appearance = Aparência
set-theme = Tema
set-theme-system = Seguir o sistema
set-theme-light = Claro
set-theme-dark = Escuro
set-accent = Cor de destaque
set-accent-orange = laranja
set-accent-blue = azul
set-accent-green = verde
set-accent-purple = roxo
set-textsize = Tamanho do texto
set-small = Pequeno
set-normal = Normal
set-large = Grande
set-density = Densidade da lista
set-compact = Compacta
set-comfortable = Confortável
set-fileicons = Ícones de arquivo
set-plainicons = Ícones simples (sem cores por tipo de arquivo)
set-animations = Animações
set-animations-hint = Desativa a seta e outras transições.
set-motion-off = Não
set-language = Idioma
set-language-system = Seguir o sistema
set-language-beta = { $name } (Beta)
set-language-note = As traduções marcadas como Beta são rascunhos automáticos que nenhum falante nativo revisou ainda. Textos ainda não traduzidos aparecem em inglês.
set-opening = Ao abrir um arquivo
set-names = Nomes de arquivo
set-names-cp949 = Coreano (CP949)
set-names-cp1252 = Latin-1 (como o GRF Editor os mostra)
set-names-utf8 = UTF-8
set-layout = Layout da lista
set-as-last = Como na última vez
set-next-start = Vale a partir da próxima vez que o aplicativo iniciar.
set-preview = Visualização
set-startup = Inicialização
set-reopen = Abrir novamente o último arquivo
set-changes = Alterações
set-warn-discard = Perguntar antes de descartar alterações pendentes
set-audio = Som
set-autoplay = Reproduzir sons automaticamente ao selecionar
set-building = Criar arquivos
set-compression = Compressão
set-format = Formato
set-building-note = Estas são apenas as opções iniciais da caixa “Criar…”; lá você pode mudá-las para cada arquivo.
set-stored-note = As configurações ficam guardadas só neste aplicativo. Atalho: { $key }+, abre esta janela.
set-reset = Restaurar padrões

method-zlib = zlib (todos os clientes)
    .hint = O padrão. Todos os clientes conseguem ler e a criação é rápida. Escolha este, a menos que tenha um motivo para não fazê-lo.
method-zopfli = zopfli (menor, lento)
    .hint = Grava o mesmo formato zlib, então todos os clientes ainda conseguem ler, mas comprime um pouco mais os arquivos. A criação demora bem mais; a leitura é igualmente rápida.
method-lzma = LZMA (precisa de suporte do cliente)
    .hint = Menor que o zlib, mas só um cliente com suporte a LZMA consegue ler (por exemplo, um com uma cps.dll). Teste o arquivo no seu cliente antes de compartilhar.
format-0x200 = 0x200 (todos os clientes)
    .hint = O formato GRF padrão. Todos os clientes leem. As posições dos arquivos usam 32 bits, então o arquivo não pode passar de 4 GB.
format-0x300 = 0x300 (acima de 4 GB)
    .hint = Formato mais novo com posições de 64 bits, então o arquivo pode ter mais de 4 GB. Só clientes mais novos leem (medido: um cliente de 2025 lê, os de 2020 não). Use só quando 4 GB não bastarem.
format-0x103 = 0x103 (clientes muito antigos)
    .hint = Formato muito antigo, anterior ao 0x200: a tabela de arquivos não é compactada e os nomes e dados são embaralhados com a variante antiga do DES. Para clientes antigos demais para ler 0x200. Mesmo limite de 4 GB.
format-0x102 = 0x102 (clientes muito antigos)
    .hint = Igual ao 0x103 (só diferem no número da versão). Use se um cliente antigo pedir exatamente esta versão.

menu-file = Arquivo
menu-edit = Editar
menu-view = Exibir
menu-window = Janela
menu-help = Ajuda
menu-settings = Configurações...
menu-open = Abrir...
menu-build = Criar a partir de pasta...
menu-merge = Mesclar...
menu-extract = Extrair...
menu-verify = Verificar arquivo
menu-reduce-images = Reduzir imagens...
menu-add = Adicionar arquivo...
menu-replace = Substituir...
menu-remove = Remover
menu-layout-tree = Como árvore
menu-layout-folders = Como pastas
menu-layout-details = Como detalhes
menu-layout-gallery = Como galeria
menu-preview-right = Visualização à direita
menu-preview-bottom = Visualização abaixo
menu-preview-off = Ocultar visualização

## Dialogs
dlg-choose-target = Escolher destino…
dlg-open-it = Abrir
dlg-add = Adicionar
job-starting = Iniciando …
job-cancelling = Cancelando …
job-progress = { $done } de { $total }
nothing-written = Nada foi gravado.
verified-yes = verificado
verified-no = não verificado
build-pick = Pasta a empacotar (a que contém data\ …)
build-title = Criar arquivo
build-pack = Empacotar { $src }
build-share = Compartilhar arquivos idênticos
build-share-hint = Guarda só uma vez arquivos de conteúdo idêntico, o que economiza espaço. Não é possível em 0x102 e 0x103.
build-verify-note = O arquivo pronto é verificado em relação aos arquivos do disco.
build-save-as = Salvar arquivo como
build-running = Criando arquivo
build-status = Criando { $dst } …
build-done-status = { $dst } criado: { $files } arquivos
build-result = Arquivos: { $files } em { $blocks } blocos ({ $from } → { $to }), { $verified }.
build-shared = Arquivos idênticos que compartilham um bloco: { $n }.
build-skipped = Itens ignorados: { $n }
build-done-title = Arquivo criado
build-cancelled = Criação cancelada
build-failed = A criação falhou
build-could-not = Não foi possível criar
merge-pick = Arquivos a mesclar (pelo menos dois)
merge-need-two = Escolha pelo menos dois arquivos.
merge-title = Mesclar arquivos
merge-higher = Prioridade maior
merge-lower = Prioridade menor
merge-intro = Ordem = prioridade: para um caminho presente em vários arquivos, vale o de cima. Os arquivos de origem não são alterados.
merge-replace-only = Só substituir: o último arquivo é a base e os outros não adicionam caminhos novos
merge-save-as = Salvar o arquivo mesclado como
merge-running = Mesclando arquivos
merge-status = Mesclando em { $dst } …
merge-done-status = Mesclado em { $dst }: { $files } arquivos
merge-line = Arquivos: { $files }, usados: { $used }, substituídos: { $replaced }
merge-line-ignored = , caminhos novos ignorados: { $n }
merge-result = Arquivos: { $files }, { $size }, { $verified }.
merge-damaged = Blocos danificados copiados sem alteração: { $n }.
merge-done-title = Arquivos mesclados
merge-cancelled = Mesclagem cancelada
merge-failed = A mesclagem falhou
merge-could-not = Não foi possível mesclar
ext-title = Extrair
ext-folder = Pasta ({ $n } arquivos)
ext-entry = Entrada selecionada
ext-all = Todos os { $n } da lista
ext-filtered = A lista está filtrada; “todos” significa as entradas que correspondem.
ext-whole = A lista mostra o arquivo inteiro.
ext-dest = Extrair para a pasta
ext-empty-folder = Esta pasta não contém arquivos.
ext-running = Extraindo …
ext-done-status = { $files } arquivo(s) extraído(s) para { $dest }
ext-done-title = Extraído
ext-done-body = { $files } arquivo(s) gravado(s) em { $dest }.
ext-damaged = { $n } entradas estavam danificadas; a parte recuperável foi gravada.
ext-stopped = Extração interrompida
ext-could-not = Não foi possível extrair
ext-rename = Renomear os arquivos novos
replace-pick = Arquivo que substitui { $name }
replace-status = Substituição pendente para { $name }; salve-a com “Salvar…”
replace-failed = Não foi possível deixar a substituição pendente
add-pick = Arquivo a adicionar
add-title = Adicionar arquivo
add-prompt = Caminho dentro do arquivo (um caminho existente é substituído):
add-status = { $path } pendente; salve com “Salvar…”
add-failed = Não foi possível deixar o arquivo pendente
remove-title = Remover do arquivo salvo
remove-body = Nada é apagado agora: as entradas ficam de fora quando você salva as alterações como um novo arquivo.
remove-status = Entradas marcadas para remoção: { $n }
remove-failed = Não foi possível deixar a remoção pendente
changes-title = Alterações
changes-none = Nenhuma alteração pendente. Use “Substituir…”, “Adicionar arquivo…” ou “Remover”; o arquivo aberto nunca é modificado, o resultado é salvo como um novo arquivo.
changes-heading = { $n ->
    [one] { $n } alteração pendente
   *[other] { $n } alterações pendentes
 }
changes-more = … e mais { $n }
changes-save = Salvar como novo arquivo…
changes-discard = Descartar tudo
changes-discarded = Alterações descartadas
save-pick = Salvar o arquivo alterado como
save-running = Salvando arquivo
save-status = Salvando { $dst } …
save-done-status = { $dst } salvo
save-result = Arquivos: { $files }, { $size }, { $verified }: adicionados { $added }, substituídos { $replaced }, removidos { $removed }.
save-note = O arquivo original não foi alterado. As alterações pendentes são mantidas até você descartá-las.
save-done-title = Arquivo salvo
save-cancelled = Salvamento cancelado
save-failed = Falha ao salvar
save-could-not = Não foi possível salvar
history-undone = Alteração desfeita
history-redone = Alteração refeita
history-failed = Não foi possível alterar o histórico

hex-title = Bytes de { $name }
hex-intro = Digite dois dígitos hexadecimais para alterar um byte. O tamanho do arquivo não muda; as alterações ficam pendentes como as demais e podem ser desfeitas.
hex-bytes-aria = Bytes do arquivo
hex-goto-ph = posição, ex.: 0x400
hex-goto-aria = Ir para a posição
hex-prev = ◀ Anterior
hex-next = Próxima ▶
hex-byte-aria = Byte na posição { $at }
hex-count = { $n ->
    [one] { $n } byte alterado
   *[other] { $n } bytes alterados
 }
hex-where = Posição { $from } – { $to } de { $size } ({ $human })
hex-apply = Aplicar alterações
hex-cannot = Não é possível mostrar os bytes
hex-status = Bytes alterados em { $name } (pendente): { $n }
hex-failed = Não foi possível alterar os bytes
hex-find-mode = Tipo de busca
hex-mode-hex = Bytes hex
hex-mode-text = Texto
hex-find-ph = buscar, ex.: 4D 5A
hex-find-btn = Buscar próximo
hex-found = Encontrado na posição { $at }
hex-not-found = Não encontrado
hex-bad-pattern = Digite dígitos hex em pares, ex.: 4D 5A
reduce-title = Reduzir imagens
reduce-level-45 = 45 dB: melhor qualidade, menor economia
reduce-level-40 = 40 dB: difícil de distinguir (recomendado)
reduce-level-38 = 38 dB: menor, leve perda em gradientes
reduce-level-35 = 35 dB: o menor, gradientes podem mostrar faixas
reduce-intro = Grava uma cópia deste arquivo em que cada BMP true-color é salvo com 256 cores, desde que a imagem mantenha a qualidade abaixo e o arquivo salvo fique menor. O arquivo aberto não é alterado. O cliente carrega BMPs de 8 bits (muitos dos seus próprios são).
psnr-hint = O PSNR mede o quanto as cores se deslocaram: a partir de 40 dB a diferença é difícil de ver; abaixo de 35 dB gradientes suaves mostram faixas.
reduce-notes = O magenta (transparente) permanece exato e nenhuma outra cor fica transparente. Imagens com no máximo 256 cores não perdem nada.
reduce-quality = Qualidade mínima
reduce-pending = As { $n } alterações pendentes também são gravadas na cópia; essas entradas não são reduzidas.
reduce-save-as = Salvar o arquivo reduzido como
reduce-running = Reduzindo imagens
reduce-status = Reduzindo imagens em { $dst } …
reduce-why-already_8_bit = já são de 8 bits
reduce-why-below_min_psnr = abaixo da qualidade
reduce-why-not_smaller = não ficam menores
reduce-why-noisy_transparency = ruído ao redor do magenta
reduce-why-alpha = com alfa
reduce-why-unreadable = ilegíveis
reduce-summary = { $reduced } de { $matched } BMPs reduzidos ({ $lossless } sem perda{ $worst }), { $saved } menor.
reduce-worst = , pior { $db } dB
reduce-kept = Mantidos como estão: { $list }.
reduce-nothing = Nada a reduzir
reduce-files = Arquivos: { $files }, { $size }, { $verified }.
reduce-done-status = Reduzido em { $dst }: { $saved } menor
reduce-done-title = Imagens reduzidas
reduce-cancelled = Redução cancelada
reduce-failed = A redução falhou
reduce-could-not = Não foi possível reduzir
reduce1-btn = 8 bits…
reduce1-btn-title = Salvar esta imagem com 256 cores (menor; o magenta continua transparente). Mostra as duas antes para comparar (pendente)
reduce1-cannot = Não é possível reduzir esta imagem
reduce1-title = Reduzir para 8 bits?
reduce1-lossless = sem perda: a imagem tem no máximo 256 cores
reduce1-psnr = PSNR { $db } dB
reduce1-info = { $before } → { $after } descompactado, { $colors } cores, { $quality }. O magenta (transparente) permanece exato.
reduce1-original = Original
reduce1-8bit = 8 bits
reduce1-go = Reduzir
reduce1-status = { $name } reduzido para 8 bits (pendente)

pvw-select = Selecione uma entrada para ver a visualização.
pvw-loading = Carregando …
pvw-meta = { $size } descompactado, { $stored } armazenado ({ $codec }), posição { $offset }
pvw-staged-removed = pendente: removido, mostrado como será salvo
pvw-staged-added = pendente: adicionado, mostrado como será salvo
pvw-staged-changed = pendente: alterado, mostrado como será salvo
pvw-hex-title = Mostrar cada byte deste arquivo e alterar bytes isolados (pendente)
pvw-files = { $n ->
    [one] { $n } arquivo
   *[other] { $n } arquivos
 }
pvw-frames = { $n ->
    [one] { $n } quadro
   *[other] { $n } quadros
 }
pvw-folder-meta = Pasta — { $files }, { $size } descompactado
pvw-open = Abrir
pvw-folder-note = “Extrair…” e “Remover” agem sobre esta pasta quando ela está selecionada.
pvw-action = Ação
pvw-action-n = Ação { $n }
pvw-anim-meta = { $frames }, { $ms } ms cada
pvw-anim-edit = Editar quadros…
pvw-anim-edit-title = Editar camadas, âncoras e quadros desta animação (pendente)
pvw-anim-interval = Definir intervalo…
pvw-anim-interval-title = Alterar o tempo por quadro desta ação (pendente, salvo com “Salvar…”)
pvw-warning = aviso: { $note }
pvw-model-hint = { $n } triângulos — arraste para girar, roda para ampliar, clique duplo para restaurar
pvw-lighting = Iluminação
pvw-lightmaps = Mapas de luz
pvw-models = Modelos
pvw-water = Água
pvw-wireframe = Wireframe
pvw-animate = Animar
pvw-all-title = Mostrar ou ocultar juntos iluminação, mapas de luz, modelos e água
pvw-hide-all = Ocultar tudo
pvw-show-all = Mostrar tudo
pvw-reset-view = Restaurar vista
pvw-reset-view-title = Voltar à vista inicial (também: clique duplo na imagem)
pvw-save-picture = Salvar imagem…
pvw-save-picture-title = Salvar a vista como está agora (com o que está desligado) como PNG, maior que na tela
pvw-extract-map = Extrair mapa…
pvw-extract-map-title = Gravar este mapa com os arquivos de que precisa em uma pasta; partes desligadas aqui podem ficar de fora
pvw-map-hint = Arraste para girar, arraste com o botão direito ou Shift, ou use as setas, para mover; roda para ampliar; clique duplo para restaurar.
pvw-effect = { $frames } mostrados a { $fps } fps
pvw-video-open = Abrir com o player do sistema
pvw-video-open-title = Gravar o arquivo em uma pasta temporária e abri-lo com o programa que o seu sistema tem para ele
pvw-video-opened = { $path } aberto. Se nada iniciou, seu sistema não tem player para este formato (VLC e RAD Video Tools reproduzem Bink).
pvw-video-info = Vídeo Bink, { $w } × { $h }, { $frames } a { $fps } fps ({ $secs } s), { $size }.
pvw-video-note = Esta janela não consegue reproduzir Bink, um formato próprio. Ele permanece inalterado no arquivo.
pvw-hex-head = { $n } bytes, os primeiros 512 exibidos
sp-cannot = Não é possível salvar a imagem
sp-not-ready = A vista 3D ainda não está pronta.
sp-scale-1 = Como na tela
sp-scale-2 = 2 × (recomendado)
sp-scale-4 = 4 ×
sp-title = Salvar imagem
sp-intro = A vista como está agora: mesmo ângulo e só o que está ligado. Uma imagem maior é redesenhada nesse tamanho, não ampliada.
sp-size = Tamanho
sp-choose-file = Escolher arquivo…
sp-save-as = Salvar imagem como
sp-filter = Imagens PNG
sp-status = Imagem salva: { $path } ({ $w } × { $h }, { $size })
sp-failed = Não foi possível salvar a imagem
em-part-models = Modelos e suas texturas
em-part-water = Imagens de água
em-part-sounds = Sons
em-part-minimap = Minimapa
em-part-effects = Script de efeitos e suas texturas
em-title = Extrair mapa
em-intro = Os arquivos de mundo, terreno e altitude e as texturas do chão sempre são gravados. As partes desligadas na vista começam desmarcadas aqui. Os arquivos deixados de fora só faltam na pasta; o mapa continua citando-os, a menos que você os tire abaixo.
em-choose-folder = Escolher pasta…
em-dest = Pasta para o mapa (os arquivos ficam abaixo como data\...)
em-running = Extraindo o mapa …
em-exists-title = Os arquivos já existem
em-exists-body = { $error }. Manter ambos, com um número adicionado aos nomes dos arquivos novos?
em-keep-both = Manter ambos
em-nothing = Nada extraído
em-status = Mapa { $map }: { $files } gravados em { $dest }
em-body = { $files } de { $map } gravados em { $dest }.
em-skipped = { $files } deixados de fora ({ $parts }).
em-missing = { $files } que o mapa cita não estão neste arquivo (podem estar em outro GRF do cliente):
em-damaged = { $files } estão danificados e foram gravados até onde foi possível ler.
em-done-gaps = Mapa extraído, com lacunas
em-done = Mapa extraído
em-failed = Não foi possível extrair o mapa

open-status = { $name }: { $format }, { $files }, { $size }

act-title = Editar animação: { $name }
act-frame-label = Quadro { $n } / { $total }
act-play = Reproduzir
act-stop = Parar
act-play-title = Reproduzir a ação no tempo de cada quadro, com os sons dos quadros
act-ms = ms/quadro
act-copy-frame = Copiar quadro
act-copy-frame-title = Inserir uma cópia depois deste quadro
act-inbetween = Intermediários…
act-inbetween-title = Adicionar quadros que se misturam suavemente de um para o próximo
act-frame-earlier = Quadro ←
act-frame-earlier-title = Mover o quadro uma posição para trás
act-frame-later = Quadro →
act-frame-later-title = Mover o quadro uma posição para frente
act-copy-action = Copiar ação
act-copy-action-title = Inserir uma cópia desta ação depois dela
act-action-up = Ação ↑
act-action-up-title = Mover a ação uma posição para cima
act-action-down = Ação ↓
act-action-down-title = Mover a ação uma posição para baixo
act-remove-action = Remover ação
act-remove-frame = Remover quadro
act-stage-role = editor de quadros
act-stage-aria = Quadro { $n }. Clique em uma camada para selecioná-la e arraste para movê-la; as setas movem a camada selecionada 1 pixel, com Shift 10.
act-stage-sel = A camada { $n } está selecionada.
act-anchor-title = Âncora { $n }: arraste para movê-la
act-no-picture = Sem imagem: { $error }
act-stage-note = As linhas marcam a origem; pontos vermelhos são âncoras. As camadas são desenhadas em ordem, a última por cima.
act-th-type = Tipo
act-th-image = Imagem
act-th-mirror = Espelhar
act-th-scale = Escala
act-th-scale-xy = Escala x / y
act-th-rot = Rot.
act-th-colour = Cor
act-type-palette = paleta ({ $n })
act-type-rgba = cores reais ({ $n })
act-layer-select = Selecionar esta camada (arraste-a na imagem ou use as setas lá)
act-layer-earlier = Desenhar antes (embaixo)
act-layer-later = Desenhar depois (em cima)
act-layer-remove = Remover a camada
act-add-layer = + Camada
act-anchors-heading = Âncoras (onde outros sprites se encaixam, ex.: chapéus)
act-add-anchor = + Âncora
act-no-anchors = ACT { $version } não tem âncoras.
act-sound = Som
act-sound-none = nenhum
act-sound-title = Arquivo de som tocado com este quadro, ex.: effect\hit.wav (vazio = nenhum)
act-sound-play-title = Reproduzir este som (data\wav\ no arquivo)
act-sound-play-aria = Reproduzir o som deste quadro
act-interval-title = Intervalo da ação { $n }
act-interval-prompt = Tempo por quadro em milissegundos (o arquivo guarda múltiplos de 25 ms):
act-interval-set = Definir
act-interval-status = Intervalo da ação { $n } pendente; salve com “Salvar…”.
act-interval-failed = Não foi possível definir o intervalo
act-remove-title = Remover a ação { $n }?
act-remove-body = As ações seguintes sobem uma posição. O cliente escolhe as ações por número (direção, estado), então outros arquivos que as usam podem mudar de significado.
ip-title = Adicionar quadros intermediários
ip-intro = Adiciona quadros que movem posição, escala, rotação, cor e âncoras em etapas de um quadro para o próximo. A imagem e o espelhamento mudam na metade, e os sons ficam nos quadros originais.
ip-count = Quadros intermediários por par
ip-whole = Ação inteira (todos os { $frames } quadros), não só o quadro { $frame } e o próximo
ip-smooth = Suave: lento no início e no fim de cada passo
ip-loop = Loop: o último quadro também se mistura de volta ao primeiro
ip-keep = Manter a velocidade: encurtar o tempo por quadro para que a ação dure o mesmo (só ação inteira)
ip-last = Este é o último quadro: sem “Loop” ou “Ação inteira” não há próximo quadro para misturar.
ip-go = Adicionar quadros
ref-title = Adicionar uma animação de referência
ref-intro = Outra animação deste arquivo desenhada junto com a que você edita, ex.: um corpo e uma cabeça para ajustar chapéus. A primeira âncora dela encontra a primeira âncora do quadro editado, como o cliente prende partes ao corpo. Mesmo número de ação e quadro; a referência só é exibida, nunca alterada.
ref-animation = Animação (.act)
ref-behind-check = Atrás da animação editada
ref-preset-body-m = Corpo, aprendiz (masculino)
ref-preset-body-f = Corpo, aprendiz (feminino)
ref-preset-head-m = Cabeça 1 (masculino)
ref-preset-head-f = Cabeça 1 (feminino)
ref-label = Referências
ref-add-btn = Adicionar referência…
ref-behind = Atrás
ref-front = Na frente
ref-toggle-title = Desenhar a referência atrás ou na frente da animação editada
ref-remove-aria = Remover a referência { $path }
te-title = Editar { $name }
te-partial = Partes deste arquivo não puderam ser decompiladas e ficam como listagem; não podem ser compiladas de novo. Edite só o que o texto mostra como Lua.
te-compile = Compilar e deixar pendente
te-stage = Deixar pendente
te-compiling = Compilando …
te-staging = Deixando pendente …
te-status = { $name } pendente. Salve com “Salvar…”.
te-status-compiled = { $name } pendente (compilado de novo). Salve com “Salvar…”.

enc-title = Arquivo criptografado
enc-body = Este arquivo está criptografado (esquema do GRF Editor). Digite a senha para ler os arquivos.
enc-wrong = Esta senha não pertence ao arquivo.
enc-skip = Abrir sem senha
enc-needs = A tabela de arquivos deste arquivo está criptografada; é preciso uma senha para abri-lo.
open-another-title = Abrir outro arquivo?
open-another-body = { $n ->
    [one] { $n } alteração pendente do arquivo aberto ainda não foi salva em um arquivo. Ela é mantida como rascunho e oferecida de novo na próxima vez que você abrir este arquivo.
   *[other] { $n } alterações pendentes do arquivo aberto ainda não foram salvas em um arquivo. Elas são mantidas como rascunho e oferecidas de novo na próxima vez que você abrir este arquivo.
 }
open-running = Abrindo { $path } …
open-failed-status = Não foi possível abrir o arquivo
open-failed = Não foi possível abrir
draft-title = Restaurar as alterações não salvas?
draft-body = { $n ->
    [one] { $n } alteração de { $when } não foi salva em um arquivo. Ela foi mantida como rascunho (por exemplo, porque o aplicativo foi fechado).
   *[other] { $n } alterações de { $when } não foram salvas em um arquivo. Elas foram mantidas como rascunho (por exemplo, porque o aplicativo foi fechado).
 }
draft-archive-changed = O próprio arquivo mudou desde então. Confira as alterações antes de salvar.
draft-missing = { $n ->
    [one] { $n } arquivo de substituição não existe mais no disco e será deixado de fora.
   *[other] { $n } arquivos de substituição não existem mais no disco e serão deixados de fora.
 }
draft-more = { $n ->
    [one] Segue { $n } rascunho mais antigo deste arquivo, um a cada vez que você o abrir.
   *[other] Seguem { $n } rascunhos mais antigos deste arquivo, um a cada vez que você o abrir.
 }
draft-later-note = “Depois” mantém este rascunho para outra hora; as alterações que você fizer agora são salvas como um rascunho próprio.
draft-restore = Restaurar
draft-discard = Descartar
draft-later = Depois
draft-restored = { $n ->
    [one] { $n } alteração restaurada
   *[other] { $n } alterações restauradas
 }
draft-restored-skipped = { $restored }, { $skipped } deixados de fora
draft-discarded = Rascunho descartado
draft-kept = O rascunho foi mantido para outra hora
draft-failed = Não foi possível restaurar as alterações
txt-encoding = Codificação:
txt-shortened = (reduzido)
txt-edit = Editar…
txt-edit-title = Editar o texto; o Lua compilado é compilado de novo (pendente, salvo com “Salvar…”)
pal-adjust = Ajustar…
pal-adjust-title = Deslocar o matiz e escalar a saturação e o brilho de uma faixa de cores (pendente)
pal-click-hint = Clique em uma cor para alterá-la.
pal-click-title = Clique em uma cor para alterá-la
pal-colour = Cor { $n }
pal-zero = O índice 0 é a cor transparente no cliente; a cor dele raramente é vista.
pal-number = Cor número { $n } de 255:
pal-changed = Cor { $n } de { $name } alterada (pendente); salve com “Salvar…”.
pal-read-failed = Não foi possível ler a paleta
pal-adjust-heading = Ajustar as cores de { $name }
pal-range-note = A cor 0 é a transparente, então a faixa começa em 1.
pal-from = Da cor
pal-to = até
pal-hue = Deslocamento de matiz (°)
pal-sat = Saturação ×
pal-bri = Brilho ×
dlg-apply = Aplicar
pal-adjusted = Cores de { $name } ajustadas (pendente); salve com “Salvar…”.
pal-failed = Não foi possível alterar a paleta
pal-pick = Paleta para { $name }
pal-filter = Paleta
pal-set-status = Paleta de { $name } alterada (pendente); salve com “Salvar…”.
pal-set-failed = Não foi possível definir a paleta
spr-add = Adicionar imagem…
spr-add-title = Anexar um PNG como nova imagem (pendente)
spr-setpal = Definir paleta…
spr-setpal-title = Usar as cores de um arquivo .pal (pendente)
spr-pixels = Pixels…
spr-pixels-title = Desenhar nesta imagem: lápis, borracha, preenchimento e conta-gotas com as cores da paleta (pendente)
spr-replace = Substituir…
spr-replace-title = Substituir esta imagem por um PNG (pendente, salvo com “Salvar…”)
spr-remove-title = Remover esta imagem; a animação ao lado acompanha (pendente)
spr-replace-pick = PNG para a imagem { $frame } de { $name }
spr-png-filter = Imagem PNG
spr-replace-status = Imagem { $frame } de { $name } pendente (as cores são mapeadas para a paleta do sprite); a visualização mostra o resultado. Salve com “Salvar…”.
spr-replace-failed = Não foi possível substituir a imagem
spr-add-pick = PNG a adicionar a { $name }
spr-add-status = Imagem adicionada a { $name } (pendente); salve com “Salvar…”.
spr-add-failed = Não foi possível adicionar a imagem
spr-remove-heading = Remover a imagem { $n }?
spr-remove-body = As imagens seguintes sobem uma posição. As camadas da animação ao lado deste sprite que usam a imagem ficam vazias, e as seguintes acompanham.
spr-remove-status = Imagem { $frame } de { $name } removida (pendente); salve com “Salvar…”.
spr-remove-failed = Não foi possível remover a imagem
pix-error = Pixels
pix-canvas-aria = Imagem { $frame }, { $w } por { $h } pixels. As setas movem o cursor, a barra de espaço desenha.
pix-size = { $w } x { $h } px
pix-readout = { $w } x { $h } px — x { $x }, y { $y }: cor { $c } (#{ $hex })
pix-readout-t = { $w } x { $h } px — x { $x }, y { $y }: cor { $c } (#{ $hex }), transparente
pix-pencil = Lápis
pix-eraser = Borracha
pix-fill = Preencher
pix-picker = Conta-gotas
pix-zoom = Zoom
pix-grid = Grade
pix-revert = Reverter
pix-palette = Paleta
pix-palette-heading = Paleta (clique em uma cor; 0 é transparente)
pix-swatch-zero =  (transparente)
pix-title = Pixels da imagem { $frame } — { $name }
pix-nochange = Nenhum pixel foi alterado.
pix-status = Imagem { $frame } de { $name } editada (pendente); salve com “Salvar…”.
pix-failed = Não foi possível deixar os pixels pendentes
gat-alt = mapa, { $x } por { $y } células
gat-x-from = x de
gat-y-from = y de
gat-to = até
gat-type-aria = Tipo de célula
gat-type-nochange = tipo: sem alteração
gat-height-ph = sem alteração
gat-height = altura
gat-set = Definir células
gat-set-title = Definir o tipo e/ou a altura das células escolhidas (pendente, salvo com “Salvar…”)
gat-drag = Arraste no mapa para escolher células.
gat-readout = x { $xa }–{ $xb }, y { $ya }–{ $yb }: { $cells }
gat-cells = { $n ->
    [one] { $n } célula
   *[other] { $n } células
 }
gat-select-all = Selecionar tudo
gat-need = Escolha primeiro um tipo de célula e/ou uma altura.
gat-status = { $cells } de { $name } alteradas (pendente); salve com “Salvar…”.
gat-failed = Não foi possível alterar as células

webgl-missing = WebGL 2 não está disponível nesta janela.

pvw-n-empty-marker = Entrada marcadora vazia
pvw-n-sound-fail = Este som não pode ser reproduzido aqui: { $error }
pvw-n-luac-fail = Lua compilado (bytecode) que ainda não pode ser exibido: { $error }
pvw-n-palette = Paleta: 256 cores
pvw-n-palette-label = 256 cores (o índice 0 é transparente)
pvw-n-gat = GAT { $version }: { $w } x { $h } células, alturas de { $lo } a { $hi }
pvw-n-gat-type = tipo { $k } ({ $name }): { $cells }
pvw-n-gnd = GND { $version }: { $w } x { $h } células, { $tiles }, { $lightmaps }, { $textures }
pvw-n-rsw = RSW { $version }: chão { $ground }, { $models }, { $lights }, { $sounds }, { $effects }
pvw-n-instances = { $instances } de { $models }
pvw-n-models-missing = { $n } não encontrados: { $names }
pvw-n-textures = { $n }
pvw-n-textures-missing = { $n ->
    [one] falta { $n } textura
   *[other] faltam { $n } texturas
 }
pvw-n-rsm = RSM { $version }: { $nodes }, { $triangles }, { $textures }
pvw-n-rsm-loop = A animação se repete a cada { $secs } s
pvw-n-node = nó { $name } ({ $v }, { $f })
pvw-n-node-moves = nó { $name } ({ $v }, { $f }), se move
pvw-n-str = STR: { $fps } fps, { $frames }, { $layers }, { $textures }
pvw-n-str-step = Apenas um a cada { $step } quadros é exibido
pvw-n-str-missing = texturas ausentes ({ $n }):
pvw-n-str-simple = Pré-visualização simplificada: modos de mesclagem e animação de chaves como no cliente, sem iluminação, em fundo escuro.
pvw-n-spr = SPR { $version }: { $pal }, { $rgba }
pvw-n-warning = aviso: { $text }
pvw-n-spr-limit = exibindo as primeiras { $shown } de { $total } imagens
pvw-n-act = ACT { $version }: { $actions }, { $frames }, { $layers }
pvw-n-act-nospr = O .spr correspondente não está neste arquivo, então não há nada para desenhar.
pvw-n-act-none = A animação não tem ações
pvw-n-act-empty = A ação { $action } não desenha nada
gat-tn-0 = transitável
gat-tn-1 = não transitável
gat-tn-2 = não transitável, não atingível
gat-tn-3 = transitável (água)
gat-tn-4 = desconhecido
gat-tn-5 = não transitável, atingível
gat-tn-6 = transitável
gat-tn-weird = estranho (bit alto definido)

un-tiles = { $n ->
    [one] { $n } bloco
   *[other] { $n } blocos
 }
un-lightmaps = { $n ->
    [one] { $n } mapa de luz
   *[other] { $n } mapas de luz
 }
un-textures = { $n ->
    [one] { $n } textura
   *[other] { $n } texturas
 }
un-models = { $n ->
    [one] { $n } modelo
   *[other] { $n } modelos
 }
un-lights = { $n ->
    [one] { $n } luz
   *[other] { $n } luzes
 }
un-sounds = { $n ->
    [one] { $n } som
   *[other] { $n } sons
 }
un-effects = { $n ->
    [one] { $n } efeito
   *[other] { $n } efeitos
 }
un-nodes = { $n ->
    [one] { $n } nó
   *[other] { $n } nós
 }
un-triangles = { $n ->
    [one] { $n } triângulo
   *[other] { $n } triângulos
 }
un-vertices = { $n ->
    [one] { $n } vértice
   *[other] { $n } vértices
 }
un-faces = { $n ->
    [one] { $n } face
   *[other] { $n } faces
 }
un-palimg = { $n ->
    [one] { $n } imagem com paleta
   *[other] { $n } imagens com paleta
 }
un-rgbaimg = { $n ->
    [one] { $n } imagem em cores reais
   *[other] { $n } imagens em cores reais
 }
un-actions = { $n ->
    [one] { $n } ação
   *[other] { $n } ações
 }
un-layers = { $n ->
    [one] { $n } camada
   *[other] { $n } camadas
 }
un-modelinst = { $n ->
    [one] { $n } instância
   *[other] { $n } instâncias
 }
un-diffmodels = { $n ->
    [one] { $n } modelo diferente
   *[other] { $n } modelos diferentes
 }

err-target-open = O destino não pode ser o arquivo aberto
err-alpha = A imagem tem um canal alfa, que BMPs de 8 bits não conseguem manter
err-saved-png = A imagem é salva como .png
err-not-png = Não é uma imagem PNG
err-no-palette = O sprite não tem paleta
err-interval-positive = O intervalo deve ser um número positivo de milissegundos
err-merge-two = Escolha pelo menos dois arquivos
err-merge-target = O destino não pode ser um dos arquivos de entrada
err-empty-marker = Esta entrada é um marcador vazio
err-enter-path = Digite o caminho dentro do arquivo, p. ex. data\texture\a.bmp
err-name-chars = O caminho tem caracteres que este modo de nomes não consegue gravar
err-marked-removal = A entrada está marcada para remoção
err-no-spr = O .spr correspondente não está neste arquivo
err-no-changes = Não há alterações para salvar
err-entry-encrypted = A entrada está criptografada (a descriptografia ainda não é suportada)
err-map-large = O mapa é grande demais
err-search-len = A busca precisa de 1 a 256 bytes
err-no-byte = Nenhum byte foi alterado
err-no-draft = Não há rascunho para este arquivo
err-already-bits = A imagem já tem { $bits } bits por pixel
err-not-file = { $path } não é um arquivo
err-file-4gib = { $path } é maior que 4 GiB
err-text-chars = O texto tem caracteres que { $encoding } não consegue representar
err-preview-large = { $mb } MB é grande demais para a pré-visualização
err-map-part = “{ $part }” não é uma parte do mapa que possa ser omitida
err-number = { $what } deve ser um número razoável
err-act-keep-action = Uma animação mantém pelo menos uma ação
err-act-no-action = Essa ação não existe
err-act-interval = O intervalo deve estar entre 1 e 60000 ms
err-act-no-frame = Esse quadro não existe
err-act-keep-frame = Uma ação mantém pelo menos um quadro
err-act-no-frame-n = O quadro { $n } não existe
err-act-image-number = O número da imagem é -1 (vazio) ou maior
err-act-image-type = O tipo de imagem é 0 (paleta) ou 1 (cores reais)
err-act-one-scale = Esta versão do ACT tem uma única escala para as duas direções
err-act-too-many-layers = Camadas demais
err-act-no-layer-n = A camada { $n } não existe
err-act-no-layer = Essa camada não existe
err-act-no-anchors = Esta versão do ACT não tem âncoras
err-act-too-many-anchors = Âncoras demais
err-act-no-anchor-n = A âncora { $n } não existe
err-act-no-frames = A ação não tem quadros
err-gat-need = Escolha um tipo e/ou uma altura para definir
err-gat-height = A altura deve ser um número razoável
err-gat-type = O tipo de célula é de 0 a 6
err-pal-range = Os números de cor vão de 0 a 255
err-pal-order = A primeira cor vem depois da última
err-pal-numbers = Matiz, saturação e brilho devem ser números
err-pal-factors = Saturação e brilho são fatores de pelo menos 0
err-spr-size = A imagem tem { $w } x { $h } pixels, os dados têm { $n } valores
err-no-archive = Nenhum arquivo está aberto
err-job-running = Já há outra tarefa em execução

menu-new = Novo
menu-new-grf = Arquivo GRF...
menu-new-gpf = Patch GPF...
menu-new-rgz = Patch RGZ...
menu-new-thor = Patch Thor...
menu-new-window = Nova janela
menu-recent = Abrir recente
menu-recent-none = Nenhum arquivo recente
menu-recent-clear = Limpar menu
menu-close-archive = Fechar arquivo
menu-save = Salvar
menu-save-as = Salvar como...
menu-save-advanced = Salvamento avançado
menu-save-repack = Reempacotar (recomprimir)...
menu-save-compact = Compactar (copiar dados)...
menu-save-encrypt = Criptografar...
menu-save-decrypt = Salvar descriptografado...
menu-archive-settings = Configurações do arquivo...
menu-changes = Revisar alterações...
menu-tools = Ferramentas
menu-hash = Hashes...
menu-compare = Comparar com arquivo...
menu-data-folder = Abrir pasta de dados
menu-about = Sobre o RustyGRF...
open-elsewhere = Este arquivo está aberto em outra janela.
err-open-elsewhere = O arquivo está aberto em outra janela
untitled = Sem título
new-failed = Não foi possível criar um novo arquivo
close-title = Fechar o arquivo?
close-body = { $n } alterações não salvas ficam como rascunho e serão oferecidas ao abrir o arquivo de novo.
close-anyway = Fechar
save-here = Salvar
save-as-button = Salvar como…
save-nothing = Nada para salvar: não há alterações.
save-here-done = { $name } salvo: { $files }, { $size }, verificado.
sa-title = Salvar como
sa-title-repack = Reempacotar (comprimir de novo)
sa-title-compact = Compactar
sa-title-encrypt = Criptografar
sa-title-decrypt = Salvar descriptografado
as-title = Configurações do arquivo
sa-store = Arquivos
sa-store-copy = Copiar como estão (rápido, compacta o arquivo)
sa-store-recompress = Comprimir de novo cada arquivo
sa-format-keep = Manter ({ $format })
sa-encryption = Criptografia
sa-enc-keep = Manter a senha
sa-enc-none = Remover a criptografia
sa-enc-plain = Nenhuma
sa-enc-password = Criptografar com senha
sa-enc-table = Criptografar só a tabela de arquivos (senha)
sa-enc-table-also = Criptografar também a tabela de arquivos (senha necessária para abrir)
as-table-encrypted = Tabela de arquivos criptografada
sa-password = Senha
sa-password-again = Repita a senha
sa-password-mismatch = As duas senhas são diferentes.
err-password-empty = Digite uma senha
sa-hint = “Salvar” grava o arquivo ao lado dele, verifica e então o substitui. A criptografia só funciona com GRF 0x200; o GRF Editor e clientes com a DLL dele a leem. Thor e RGZ são gravados conforme o nome de arquivo escolhido.
sa-filter = Arquivos { $type }
as-path = Arquivo
as-unsaved = ainda não salvo
as-type = Tipo
as-content = Conteúdo
as-encryption = Criptografia
as-encrypted = Senha do GRF Editor
as-plain = nenhuma
as-names = Nomes exibidos como
err-save-same = Escolha outro arquivo: “Salvar” grava no arquivo aberto
err-save-unverified = O arquivo gravado não passou na verificação; o original não mudou
err-save-replace = Não foi possível substituir { $path } ({ $error }). O arquivo salvo é { $tmp }.
err-encrypt-format = A criptografia só é possível com GRF 0x200
err-patch-empty = Um patch precisa de pelo menos um arquivo
err-crypt-format = Entradas isoladas só podem ser criptografadas em arquivos GRF da versão 0x200
err-crypt-none-plain = Todas as entradas escolhidas já estão criptografadas ou não podem ser
err-crypt-none-encrypted = Nenhuma das entradas escolhidas está criptografada
err-crypt-need-key = É preciso uma senha para criptografar: escolha Criptografar… de novo e digite-a
err-thor-target = A GRF de destino deve ser um nome de arquivo sem pasta (por exemplo data.grf)
hash-title = Hashes
hash-pick = Selecione um arquivo primeiro.
hash-size = Tamanho
cmp-pick = Comparar com qual arquivo?
cmp-running = Comparando …
cmp-failed = Não foi possível comparar
cmp-title = Diferenças
cmp-same = Mesmo conteúdo
cmp-summary = Comparado com { $other }: { $here } só aqui, { $changed } alterados, { $there } só lá, { $same } iguais.
cmp-only-here = Só neste arquivo
cmp-changed = Alterados
cmp-only-there = Só no outro arquivo
cmp-patch-hint = Um patch contém o que é novo ou alterado aqui; um patch Thor também apaga o que só está no outro arquivo.
cmp-make-patch = Salvar como patch…
cmp-patch-pick = Salvar o patch como
cmp-patch-running = Gravando o patch …
cmp-patch-done = Patch gravado
cmp-patch-result = { $added } novos, { $changed } alterados, { $removed } removidos; { $size }, gravado em { $dst }.
about-title = Sobre o RustyGRF
about-version = Versão { $version }
about-tagline = Uma ferramenta para arquivos, sprites, animações, paletas e mapas do Ragnarok Online, no macOS, Windows e Linux.
about-made-by = Feito por
about-thanks = Agradecimentos
about-tokeiburu = GRF Editor e ACT Editor, a inspiração; formatos portados com a permissão dele
about-lua = Lua 5.0 e 5.1 (MIT), para verificar e compilar arquivos Lua
about-tauri = a janela do aplicativo
about-license = Licença MIT ou Apache-2.0. Sem vínculo com a Gravity.
save-as-done = Salvo como { $dst }; esta janela o mostra agora.
save-unverified = O arquivo salvo não passou na verificação: { $dst }
menu-search-contents = Pesquisar nos arquivos...
sc-title = Pesquisar nos arquivos
sc-query = Texto
sc-query-ph = o que procurar, p. ex. prontera
sc-files = Arquivos
sc-files-ph = *.lua;*.lub;*.txt (vazio: todos)
sc-regex = Expressão regular
sc-case = Diferenciar maiúsculas
sc-word = Palavras inteiras
sc-in-list = Só os arquivos da lista
sc-search = Pesquisar
sc-running = Pesquisando …
sc-result = { $lines } linhas em { $files } (de { $searched } pesquisados).
sc-more = Há mais linhas do que as listadas.
sc-none = Nada encontrado em { $searched }.
sc-show = Mostrar na lista
err-search-empty = Digite o texto a procurar
err-bad-pattern = A expressão regular não é válida: { $error }
menu-undo = Desfazer
menu-redo = Refazer
menu-history = Histórico de alterações...
menu-nav-back = Voltar
menu-nav-forward = Avançar
menu-text-encoding = Codificação de texto...
hist-add = Adicionado { $name }
hist-replace = Substituído { $name }
hist-edit = Editado { $name }
hist-remove = Removido { $name }
hist-discard = Descartada a alteração de { $name }
hist-mixed = Alterado { $name }
hist-add-n = Adicionados { $n } arquivos
hist-replace-n = Substituídos { $n } arquivos
hist-edit-n = Editados { $n } arquivos
hist-remove-n = Removidos { $n } arquivos
hist-discard-n = Descartadas { $n } alterações
hist-encrypt = { $name } criptografado
hist-decrypt = { $name } descriptografado
hist-encrypt-n = { $n } arquivos criptografados
hist-decrypt-n = { $n } arquivos descriptografados
hist-mixed-n = Alterados { $n } arquivos
history-undone-n = { $n } passos desfeitos
history-redone-n = { $n } passos refeitos
hist-title = Histórico de alterações
hist-redo-head = Pode ser refeito
hist-now = ▸ Agora
hist-undo-head = Pode ser desfeito (mais recente primeiro)
hist-empty = Nada para desfazer.
hist-hint = Um clique volta (ou avança) até esse passo. Até 200 passos por arquivo; o histórico termina ao salvar, e as alterações sobrevivem a uma falha como rascunho. Clique direito em Desfazer ou Refazer mostra a mesma lista.
te-title = Codificação de texto
te-intro = Texto lido na codificação errada (por exemplo, um nome coreano exibido como Latin-1) fica legível: ele é escrito de volta na codificação em que foi exibido e lido na correta.
te-source = Texto como exibido
te-result = Resultado
te-from = Exibido como
te-to = Ler como
te-swap = Inverter
te-bytes = Bytes: { $hex }
te-lossy = Alguns caracteres não existem em uma das codificações e foram substituídos.
as-magic = Cabeçalho mágico
as-magic-reset = Padrão
sa-thor-merge = Mesclar em uma GRF
sa-thor-target = GRF de destino
sa-thor-target-ph = vazio: a padrão do cliente
sa-thor-hint = Um patch Thor grava seus arquivos na pasta do cliente ou os mescla em uma GRF. O destino é o nome de arquivo dessa GRF.
err-bad-magic = O cabeçalho não é válido: até 16 caracteres ASCII ({ $error })
menu-flat-maps = Mapas planos...
pvw-flat-map = Alterar mapa…
pvw-flat-map-title = Chão plano, sem sombras, luz, objetos ou água, neste arquivo (Desfazer reverte)
flat-flatten = Chão plano (sem morros nem paredes; também sem objetos e sem água)
flat-remove_shadow = Remover sombras
flat-remove_light = Remover luz colorida
flat-remove_color = Remover cores dos ladrilhos
flat-gat_shadows = Sombras da grade de passagem (células bloqueadas escuras; substitui luz e sombras)
flat-reset_light = Luz do sol neutra
flat-remove_models = Remover modelos
flat-remove_objects = Remover todos os objetos (modelos, luzes, sons, efeitos)
flat-remove_water = Remover água
flat-notes = As regras seguem o criador de mapas planos do GRF Editor. As versões dos arquivos não mudam; a água é movida para baixo do chão. A grade de passagem (por onde se pode andar) não muda.
flat-none = Nada está marcado.
flat-failed = Não foi possível alterar o mapa
flat1-title = Alterar mapa
flat1-intro = Os arquivos de mundo, terreno e altitude deste mapa são alterados no arquivo aberto. Salve para manter a alteração; Desfazer reverte. As partes desligadas na vista já vêm marcadas.
flat1-go = Alterar
flat1-status = Mapa { $map } alterado (ainda não salvo)
flats-title = Mapas planos
flats-intro = Grava um arquivo novo e pequeno só com os arquivos de mapa alterados. Coloque-o antes dos outros no DATA.INI e o cliente usa estes mapas; o arquivo aberto não muda.
flats-selected = Mapa selecionado
flats-in-folder = Mapas da pasta selecionada
flats-all = Todos os mapas
flats-pending = As alterações pendentes nos arquivos de mapa ({ $n } alterações no total) são usadas.
flats-save-as = Salvar os mapas planos como
flats-running = Alterando mapas
flats-summary = Mapas alterados: { $changed } de { $maps }.
flats-problems = Deixados de fora ({ $n }):
flats-nothing = Nada mudou, nada foi gravado
flats-written = { $files } gravados em { $dst } ({ $size }).
flats-how = Para usar, adicione-o no DATA.INI antes dos outros arquivos (por exemplo como 0=).
flats-status = { $changed } mapa(s) plano(s) gravado(s) em { $dst }
flats-done = Mapas planos gravados
flats-cancelled = Mapas planos cancelados
em-change = Alterar os arquivos do mapa
em-strip = Tirar também do arquivo de mundo as partes deixadas de fora acima (para o cliente não procurá-las)
em-changed = { $files } gravados com alterações.
err-flat-not-map = Não é um arquivo de mapa (.rsw, .gnd ou .gat)
err-flat-no-gnd = O arquivo de terreno { $name } não está neste arquivo
err-flat-failed = O mapa não pode ser alterado: { $error }
err-flat-nothing = Estas opções não mudam nada neste mapa
ctx-open = Abrir
ctx-opened = Cópia aberta: { $path }
ctx-reveal = Mostrar no { $app }
ctx-fm = gerenciador de arquivos
ctx-extract = Extrair…
ctx-extract-last = Extrair para “{ $folder }”
ctx-extract-folder = Extrair pasta…
ctx-export = Exportar para um arquivo…
ctx-export-folder = Exportar pasta para um arquivo…
ctx-map-extract = Extrair mapa…
ctx-edit = Editar texto…
ctx-hex = Visão hexadecimal…
ctx-reduce = Reduzir para 8 bits…
ctx-replace = Substituir por arquivo…
ctx-add-here = Adicionar arquivo aqui…
ctx-rename = Renomear…
ctx-rename-folder = Renomear pasta…
ctx-delete = Excluir
ctx-delete-folder = Excluir pasta
ctx-usages = Buscar o nome nos arquivos…
ctx-copy-name = Copiar nome
ctx-copy-path = Copiar caminho
ctx-props = Propriedades…
ctx-encrypt = Criptografar…
ctx-decrypt = Descriptografar
crypt-title = Criptografar entradas
crypt-intro = As entradas são criptografadas pelo método do GRF Editor ao salvar. Um cliente só as lê com a mesma senha (e uma cps.dll que a conheça).
crypt-go = Marcar para criptografar
crypt-status-on = Será criptografado ao salvar: { $files }
crypt-status-off = Será descriptografado ao salvar: { $files }
crypt-failed = Não foi possível marcar as entradas
ctx-copied = Copiado: { $text }
ren-title = Renomear arquivo
ren-title-folder = Renomear pasta
ren-intro = O novo caminho deste arquivo no arquivo GRF. Outra pasta o move para lá.
ren-intro-folder = O novo caminho desta pasta; todos os arquivos dentro dela vão junto.
ren-hint = É salvo como remoção do caminho antigo e adição do conteúdo no novo; Desfazer reverte. Pastas grandes (acima de 512 MB) não podem ser renomeadas assim.
ren-go = Renomear
ren-status = { $from } renomeado para { $to } (ainda não salvo)
ren-failed = Não foi possível renomear
err-rename-nothing = Não há nada para renomear
err-rename-large = Ao renomear, os arquivos ficam na memória; mais de { $mb } MB é demais. Extraia a pasta e adicione-a de novo.
err-rename-mismatch = { $name } não está sob o caminho que está sendo renomeado
err-rename-exists = { $name } já existe
err-export-exists = { $path } já existe
exp-title = Exportar para um arquivo
exp-intro = { $n } arquivo(s) viram um novo arquivo GRF; o arquivo aberto não muda.
exp-method = Compressão
exp-m-zlib = zlib (funciona em qualquer cliente)
exp-m-lzma = LZMA (exige a cps.dll com suporte a LZMA)
exp-m-zopfli = Zopfli (compatível com zlib, menor, lento)
exp-note = O conteúdo é tomado como está salvo no arquivo; as alterações pendentes não entram.
exp-choose = Escolher arquivo…
exp-save-as = Salvar o novo arquivo como
exp-running = Exportando
exp-done = Arquivo gravado
exp-done-body = { $files } gravados em { $dst } ({ $size }, { $verified }).
exp-status = { $n } arquivo(s) exportado(s) para { $dst }
exp-failed = Não foi possível exportar
prop-title = Propriedades
prop-path = Caminho
prop-type = Tipo
prop-size = Tamanho
prop-stored = Tamanho armazenado
prop-codec = Compressão
prop-offset = Posição no arquivo
prop-flags = Flags
prop-files = Arquivos
prop-total = Tamanho total

# sound editor
ctx-audio = Editar som…
au-wave = Forma de onda: arraste para escolher uma parte
au-title = Editar som: { $name }
au-intro = Arraste sobre a onda para escolher uma parte. As etapas valem para a seleção, se houver (cortar, recortar e silenciar exigem uma). O arquivo só muda quando você clicar em «Aplicar ao arquivo»; o resultado mantém o formato do arquivo: WAV PCM de 16 bits, que qualquer cliente reproduz; MP3 e Ogg são codificados de novo (perde-se um pouco de qualidade a cada vez).
au-info = { $rate } Hz · { $channels } · { $ms } ms · pico { $peak } dB
au-mono = mono
au-stereo = estéreo
au-channels = { $n } canais
au-sel = Seleção: { $a } – { $b } ms
au-sel-none = Sem seleção (o som inteiro)
au-sel-all = Selecionar tudo
au-g-part = Trecho
au-trim = Manter a seleção
au-cut = Recortar a seleção
au-silence = Silenciar a seleção
au-g-level = Nível
au-gain = Volume (dB)
au-ms = ms
au-apply-step = Aplicar
au-normalize = Normalizar
au-fadein = Fade in
au-fadeout = Fade out
au-g-shape = Forma
au-reverse = Inverter
au-mono-btn = Mixar em mono
au-rate = Taxa de amostragem
au-resample = Reamostrar
au-speed-f = Fator
au-speed = Mudar velocidade (e tom)
au-g-fx = Efeitos
au-echo-delay = Atraso (ms)
au-echo-fb = Repetições (0–0,95)
au-mix = Mistura (0–1)
au-echo = Adicionar eco
au-room = Sala (0–1)
au-reverb = Adicionar reverberação
au-g-filter = Filtro
au-lowpass = Passa-baixas
au-highpass = Passa-altas
au-ops = Etapas
au-ops-none = Ainda não há etapas.
au-undo = Desfazer a última etapa
au-reset = Recomeçar
au-needs-sel = Escolha primeiro um trecho do som (arraste sobre a onda).
au-apply = Aplicar ao arquivo
au-failed = Não foi possível editar o som
au-status = Som { $name } editado ({ $n } etapas), ainda não salvo
err-audio-read = Não é possível ler o som: { $error }
err-audio-write = Não é possível gravar o som: { $error }
err-audio-op = A etapa não funciona: { $error }

# map downgrade
ctx-downgrade = Rebaixar mapa…
dg-title = Rebaixar o mapa para clientes antigos
dg-intro = Os modelos RSM2 do mapa viram modelos RSM1 ao lado dos antigos, o arquivo de mundo passa para a versão 2.1 e o de terreno para 1.7 (a água vai para o arquivo de mundo), como o «Downgrade» do GRF Editor.
dg-hint = As animações dos modelos se perdem: eles ficam estáticos. Nada é gravado até você salvar; a etapa pode ser desfeita.
dg-go = Rebaixar
dg-status = { $models } modelos convertidos, { $instances } objetos alterados
dg-missing = Modelo que não está no arquivo
dg-failed = Não foi possível rebaixar o mapa
err-downgrade-failed = Não foi possível rebaixar o mapa: { $error }
err-downgrade-nothing = Este mapa não tem modelos RSM2 para converter

# tabs
tab-strip = Abas
    .aria-label = Abas
tab-empty = Nova aba
tab-close = Fechar aba
tab-new = Abrir um arquivo em uma nova aba
tab-new-window = Mover para uma nova janela
tab-merge = Reunir todas as janelas em abas desta
set-tabs = Abas
set-tabs-open = Abrir os arquivos em abas da janela (em vez de novas janelas)

# Messages of the libraries (the English text is matched in app.js, see LIB_ERRORS)
le-io = Erro de entrada/saída: { $reason }
le-not-grf = Não é um arquivo GRF: { $reason }
le-grf-version = Versão GRF 0x{ $v } sem suporte (compatíveis: 0x102, 0x103, 0x200, 0x300)
le-corrupt-table = Tabela de arquivos danificada: { $reason }
le-no-entry = A entrada número { $n } não existe
le-entry-range = A entrada { $n } está fora do arquivo (deslocamento { $offset }, tamanho { $size }, tamanho do arquivo { $file })
le-unsafe-path = Caminho inseguro ou inválido no arquivo: { $path }
le-exists = O destino já existe: { $path }
le-repack = Não é possível gravar o arquivo: { $reason }
le-wrong-key = Esta chave não pertence ao arquivo (o CRC32 difere do guardado em __encryption.info)
le-table-encrypted = A tabela de arquivos está criptografada (método do GRF Editor): a chave é necessária para abrir o arquivo
le-verify = A verificação do arquivo gravado falhou: { $reason }
le-r-magic = Falta a assinatura de cabeçalho «Master of Magic» ou «Event Horizon»
le-r-small = O arquivo tem apenas { $n } bytes
le-r-thor = Falta a assinatura Thor
le-r-table-big = A tabela de arquivos é grande demais
le-r-table-out = A tabela de arquivos está fora do arquivo
le-r-table-z = Não é possível descompactar a tabela de arquivos: { $reason }
le-r-table-size = A tabela de arquivos tem { $got } bytes, o cabeçalho diz { $want }
le-r-trailing = { $n } bytes inesperados após { $count } entradas
le-r-entry-cut = A entrada { $n } está cortada
le-r-entry-name = O nome da entrada { $n } não termina
le-sig = Não é um arquivo { $format } (falta a assinatura «{ $sig }»)
le-not-fmt = Não é um arquivo { $format }
le-early = Os dados { $format } terminam cedo demais (precisa de { $need } bytes no deslocamento { $at }, o arquivo tem { $len })
le-early-gnd = Os dados GND terminam cedo demais (precisa de { $need } bytes, o arquivo tem { $len })
le-truncated = O arquivo { $format } está cortado ou danificado
le-bytecode-cut = O bytecode está cortado ou danificado
le-not-lua = Não é bytecode Lua
le-lua-unsupported = Bytecode Lua sem suporte: { $reason }
le-version = Versão { $format } { $v } sem suporte
le-prefix-world = Arquivo de mundo: { $reason }
le-prefix-ground = Arquivo de terreno: { $reason }
le-prefix-model = Modelo: { $reason }
le-not-rsm2 = { $name } não é um modelo RSM2 (versão 2.2 ou 2.3)
le-gnd-header = O cabeçalho GND não é válido ({ $reason })
le-gat-size = O tamanho GAT { $w } x { $h } não é válido
le-gat-extra = { $n } bytes de dados não usados após as células
le-gat-cell = A célula ({ $x }, { $y }) está fora do mapa de { $w } x { $h }
le-lightmaps = O mapa precisaria de mais de 65.536 lightmaps diferentes
le-gat-shadows = As sombras da grade de caminhada precisam do arquivo GAT do mapa
le-object-type = O objeto { $n } tem o tipo desconhecido { $kind }
le-spr-no-image = O SPR não tem a imagem { $kind } { $index } ({ $count } imagens desse tipo)
le-spr-no-palette = O SPR não tem paleta (não contém imagens com paleta)
le-spr-no-true = O sprite não tem imagens em cores reais
le-spr-no-pal-img = O sprite não tem imagens com paleta
le-spr-need-pal = Imagens indexadas precisam de uma paleta
le-spr-too-many = Imagens demais (no máximo 65.535 de cada tipo)
le-spr-big = A imagem tem { $w } x { $h }, as imagens SPR têm no máximo 65.535 x 65.535
le-act-no-action = O ACT não tem a ação { $index }
le-act-no-frame = A ação { $action } não tem o quadro { $frame }
le-act-no-next = A ação { $action } não tem quadro depois do quadro { $frame } para mesclar
le-act-nothing = Nada selecionado: nenhuma camada corresponde
le-act-sound-name = O nome do som deve ter no máximo 39 caracteres Latin-1
le-wav-tag = Formato WAV sem suporte ({ $tag })
le-wav-chunks = O WAV não tem bloco de formato ou de dados utilizável
le-adpcm = Os dados ADPCM são inconsistentes
le-audio-op = Operação desconhecida «{ $name }»
le-audio-range = O intervalo { $a }–{ $b } ms está vazio ou fora do som ({ $len } ms de duração)
le-no-lossy = Sons MP3 e Ogg Vorbis só podem ser editados em uma versão com o recurso lossy-audio
le-mp3-rate = O MP3 não aceita { $rate } Hz (8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100 ou 48000); reamostre antes
le-mp3-channels = O MP3 aceita um ou dois canais
le-lossy-read = Não é um som legível: { $reason }
le-img-none = A imagem não tem pixels
le-img-size = O tamanho da imagem não corresponde aos pixels
le-tga-short = O arquivo é curto demais para uma imagem TGA
le-tga-depth = Profundidade de pixel TGA sem suporte ({ $n } bits)
le-tga-type = Tipo de imagem TGA { $n } sem suporte
le-bmp-unsupported = BMP sem suporte (compressão { $compression }, { $bits } bits por pixel)
le-bmp-size = O BMP tem um tamanho inválido
le-png-fail = Não foi possível processar o PNG ({ $how }): { $reason }
le-gif-fail = Não foi possível gravar o GIF: { $reason }
le-pal-size = Um arquivo PAL tem { $want } bytes, este tem { $got }
