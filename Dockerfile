FROM python:3.9-alpine

WORKDIR /app

COPY . /app

RUN apk --no-cache add build-base libffi-dev postgresql-dev && \
    pip install --upgrade pip && \
    pip install -r requirements.txt

EXPOSE 8080

CMD ["gunicorn", "--bind", "0.0.0.0:8080", "app:app"]
