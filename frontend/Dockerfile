# Stage 1: Build
FROM node:25-alpine AS build-stage
WORKDIR /app
COPY . .
RUN npm install
RUN npm run build

# Stage 2: Serve with Nginx
FROM nginx:stable-alpine
# Copy the static build files to Nginx's serve directory
COPY --from=build-stage /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/nginx.conf

EXPOSE 80
CMD ["nginx"]
