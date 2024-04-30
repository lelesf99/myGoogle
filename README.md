
# MyGoogle Search

Esse repositório é destinado ao código relacionado a um trabalho de sistemas distribuídos. O trabalho foi passado por meio de um documento pdf contendo os requisitos funcionais e não funcionais de uma aplicação. 

> Entrega 1 – MyGoogle Search
> ...
> Este sistema tem uma estrutura simples de uso. Há pelo menos um cliente que solicita
> ao servidor do MyGoogle Search a pesquisa de palavras chaves que devem ser buscadas
> em documentos indexados pelos servidores da solução. O serviço do MyGoogle Search
> deve permitir um cliente informar como parâmetro um conjunto de palavras chaves em
> sua requisição. Como resposta, o servidor do MyGoogle Search irá devolver uma lista de
> arquivos mais relevantes para o conjunto de palavras chaves informadas pelo cliente.
> ...
> A aplicação cliente do MyGoogle Search será disponibilizada como uma app web com um
> cliente que atende aos seguintes requisitos funcionais:
> 1 Fazer o upload de arquivos que serão indexados para posterior busca;
> 2 Remover um arquivo do sistema;
> 3 Listar todos os arquivos inseridos;
> 4 Fazer uma pesquisa de palavras chaves;
> 5 Listar na tela a resposta da pesquisa realizada.
> O cliente deve ser um cliente web simples e o servidor pode ser estruturado da forma que
> for mais apropriado.
> ...
> Linguagem de desenvolvimento:
> _ C++/C
> _ Java
> _ Python
> Data de entrega: 22/04/2024

Esses são trexos do documento passado como guia para o desenvolvimento da aplicação.

A aula de sistemas distribuídos tem grande foco em sockets e como desenvolver aplicações que usam conexões tcp udp diretamente nos sockets. Quando vi que o trabalho precisava de um cliente web fiquei curioso, trabalho com desenvolvimento web a 5 anos e nunca ouvi falar de alguma API Javascript ou qualquer coisa nativa browsers que interagem com sockets crus.

Dito e feito, pesquisar sobre isso traz uma enxurrada de resultados negativos, não existe essa API, ou experimentais. E sabendo que frameworks e soluçoes que trazem outras linguagens para o desenvolvimento web geralmente precisam ser transpiladas para JS antes para funcionar no browser eu também duvidei que fosse do intuito desse trabalho estudar sobre alguma API que transforma requisições HTTP em conexões TCP e depois conectam ao backend, tudo isso me pareceu ter um escopo gigante demais para um trabalho de 3 semanas.

"O cliente deve ser um cliente web simples e o servidor pode ser estruturado da forma que for mais apropriado."

Essa foi a frase que me guiou a partir daí. Decidi usar python + sqlite no backend, e html + javascript no front. E fui bem sucedido. Primeiro tive alguma dificuldade em escolher qual tecnologia usar, primeiro fiz tudo em http e usando Flask no backend para lidar com as requisições, mas tudo aquilo me parecia não ter nenhuma relação com a matéria até então.

Depois decidi usar websockets, que apeasar de não serem literalmente conexões TCP e sockets crus, tem um uso e arquitetura parecida. Tive alguns problemas encaixando todos os requisitos nessa arquitetura, principalmente a parte de upload e download de arquivos. 

Como esses requisitos me pareciam secundários, me frustei com o fato de que estava gastando tanto tempo nisso e decidi por fim usar http para requisições de controle de arquivos, e websockets para a requisição de pesquisa, aproveitei o fato de se tratar de uma conexão realtime para atualizar em tempo real os resultados conforme a busca era realizada.

Esse projeto está contido na pasta "web", para roda-lo é necessário instalar o python, iniciar o banco e rodar o app.py. pesquisei um pouco sobre como compiilar em um .exe para distribuição e teste mas não fui muito longe.

Na quinta feira dia 18 depois da aula dessa matéria, conversei com um colega sobre qual solução ele tinha usado para lidar com um backend em sockets e um cliente web. E ele me falou que **a documentação estava errada**, e o **cliente não era pra ser web**. Que tinha que ser uma aplicação no terminal mesmo e que ele tinha escolhido Java.

Bom.. **comecei do zero denovo**. Decidi usar Rust porque vi que ele tinha suporte nativo a gerenciamento de conexões TCP e UDP. E rust tem uma boa reputação de velocidade e bom devx. O proejto principal se tornou esse em Rust e ele está contido nas pastas client e server.

A arquitetura dele consiste em um loop que espera por uma requisição de conexão TCP, aceita a conexão e a coloca em uma nova thread para processamento. A forma das mensagens é o mecanismo de controle e padronização da comunicação entre cliente e servidor. Geralmente é assim \<comando> \<arg1> \<arg2> \<arg3> ...

Fiz o que pude. mas não consegui aetender todos os requisitos principalmente os não funcionais. Não consegui fazer a biblioteca async tokio funcionar para atender multiplos clientes. E não consegui finalizar os testes de performance. Outro todo importante é otimizar o código da pesquisa estou olhando sobre KMP que parece ser um fácil de entender. Vou passar o restante da semana trabalhando nisso.

Sobre os slides da apresentação não fiz porque nem o projeto eu finalizei então a apresentação eu ia fazer só mostrando os apps rodando e etc.

Aprendi que não posso confiar no PO e tenho que caçar pra achar os requisitos corretos. E que não é legal quando o projeto muda por inteiro 4 dias para a entrega.

Resolvi alguns problemas de leitura das mensagens, estava usando um reader que era ineficiente e não garantia a leitura imediata do buffer do TCP, isso tava causando diversos problemas. Agora só falta testar em uma maquina melhor, já que os notebooks que tenho aqui em casa ambos são meio limitados e tanto o cliente qunato o servidor depende da quantidade de threads disponíveis para manter as conexões. Ambos utilizam uma pool de workers e enfileiram as requisições, então mesmo que o teste seja configurado para tantas conexões por segundo, o cliente consegue mandar e manter algumas, a mesma coisa para o servidor, mas tecnicamente, a fila de requisições só depende da memória do servidor, então achei o suficiente. 

