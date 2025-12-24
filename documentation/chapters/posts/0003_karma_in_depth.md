# Instructions

Using this model of JSON below, create a new json with the languages pt-BR, zh-CN, en in that order inside the json, like the JSON example below.
If the Slides written in the specification below are in one language, create a JSON containing the other ones aswell. 
Create the .json file using the name specified below, in the same directory as this AI.md file. 
Then go to the main.typ in this same /posts dir and change the file used to point to the newly created one. Example:
```typst
#let data = json("0001_hello_world.json")
```

# Data to JSON

*Name*: 0003_karma_in_depth.json
*First Slide/Title*: Karma: Rules and Automation
*Slides List*:
- Karma is Lince's rule engine: If [condition] then [consequence].
- Checks data every 60 seconds (Delivery).
- It can execute system commands or change other records.
- Example: If the 'Exercise' task is -1, close distraction apps.
- Create blueprints (DNA) to automate your routine and production.

# JSON Target Schema Example

```json
{
  "posts": [
    {
      "id": "overview",
      "title": {
        "pt-BR": "O que é a Lince?",
        "zh-CN": "什么是林斯 (Lince)？",
        "en": "What is Lince?"
      },
      "slides": [
        {
          "i18n": {
            "pt-BR": "Uma ferramenta para registro, interconexão e automação de Necessidades e Contribuições.",
            "zh-CN": "一个用于需求和贡献的注册、互联和自动化的工具。",
            "en": "A tool for registry, interconnection, and automation of Needs and Contributions."
          }
        },
        {
          "i18n": {
            "pt-BR": "Sua base de dados pessoal é o seu DNA (SQLite).",
            "zh-CN": "您的个人数据库就是您的 DNA (SQLite)。",
            "en": "Your personal database is your DNA (SQLite)."
          }
        }
      ]
    }
  ]
}
```
