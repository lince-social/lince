FROM python:3.9-alpine

WORKDIR /app

COPY python_requirements.txt /app

RUN apk --no-cache add build-base libffi-dev postgresql-dev && \
        pip install --upgrade pip && \
        pip install -r python_requirements.txt

COPY . /app

EXPOSE 8080

CMD ["python", "crud.py"]
