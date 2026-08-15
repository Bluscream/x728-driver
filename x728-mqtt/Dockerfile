FROM python:3.11-slim
WORKDIR /app
RUN pip install --no-cache-dir paho-mqtt
COPY app.py /app/app.py
CMD ["python", "-u", "/app/app.py"]
