# Instructions

Using this model of JSON below, create a new json with the languages pt-BR, zh-CN, en in that order inside the json, like the JSON example below.
If the Slides written in the specification below are in one language, create a JSON containing the other ones aswell. 
Create the .json file using the name specified below, in the same directory as this AI.md file. 
Then go to the main.typ in this same /posts dir and change the file used to point to the newly created one. Example:
```typst
#let data = json("0001_hello_world.json")
```

# Data to JSON

*Name*: 0001_lince_overview.json
*First Slide/Title*: What is Lince?
*Slides List*:
- A tool for registry, interconnection, and automation of Needs and Contributions.
- We will cover in detail all the parts of Lince, for now let's see a summary.
- Your data is private and local by default, called your DNA.
- Everything revolves around Records, which can model actions, items... anything you want.
- Automations can be done using Karma, a system of IF (conditions) and THEN (consequence).
- The automations can change your Records, run programs and more, creativity is the limit.
- You can share parts of your data with others to collaborate and trade resources.
- Many Lince instances can create a group, called an Organ, a decentralized peer-to-peer network.  
- You can use the Organs to create group tasks, to buy goods and services, and more.
- Check our other posts and the YouTube channel for more information.

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
