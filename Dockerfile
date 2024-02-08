FROM alpine:latest

WORKDIR /app

COPY . /app

RUN apk add --no-cache \
        build-base libffi-dev postgresql-dev && \
        pip install --upgrade pip && \
        pip install -r python_requirements.txt && \
        npm install orbit-db pump.io

EXPOSE 8501

CMD ["python", "streamlit_crud.py"]
