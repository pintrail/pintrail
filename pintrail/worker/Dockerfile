FROM node:22-alpine

WORKDIR /app

COPY package*.json ./
RUN npm ci

COPY index.js ./

ENV IMAGE_STORAGE_ROOT=/data/images

CMD ["node", "index.js"]
