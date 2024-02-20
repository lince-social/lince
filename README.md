# Lince

Ferramenta para cadastro e conexão entre necessidades e contribuições de escopo livre.

Repositório para todo o código, a documentação geral e tecnológica pode ser acessada nesse 
[link](https://github.com/lince-social).

Instalação (ainda nao dockerizada, utilizando arch linux):
1. Instalar postgresql
2. Criar um banco de dados chamado personallince (não é necessário, mas o streamlit_crud.py tem esse nome de banco de dados como alvo de alterações então esse nome é pra dar match)
3. Rodar o script base_de_dados.sql enquanto no banco de dados personallince (\i [caminho pro script {no meu caso o comando é \i ~/lince/lince/base_de_dados.sql}])
4. Criar um ambiente virtual (boas práticas) (ex: conda create --name lince) e ativá-lo (conda activate lince)
5. Instalar, preferivelmente dentro do ambiente virtual, os pacotes de python pra rodar o streamlit_crud.py: pip install streamlit psycopg2 pandas uuid
6. Terminal: python streamlit_crud.py && streamlit run streamlit_crud.py
7. Provavelmente uma aba será aberta no broswer no localhost port 8501

