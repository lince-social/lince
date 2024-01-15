FROM alpine:latest

WORKDIR /app

COPY . /app

RUN apk --no-cache add python3

EXPOSE 8080

CMD ["python3", "app.py"]
