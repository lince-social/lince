FROM alpine:latest

WORKDIR /app

COPY . /app

RUN apk add --no-cache build-base libffi-dev postgresql-dev
RUN pip install --upgrade pip
RUN pip install psycopg2 uuid pandas streamlit

CMD ["python", "frontend.py"]
