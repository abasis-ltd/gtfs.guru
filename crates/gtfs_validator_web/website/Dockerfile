FROM nginx:alpine

# Copy static assets
COPY . /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

# Deployment inputs must not be published from the web root.
RUN rm -f /usr/share/nginx/html/nginx.conf \
          /usr/share/nginx/html/Dockerfile \
          /usr/share/nginx/html/docker-compose.yml \
          /usr/share/nginx/html/docker-compose.yaml \
          /usr/share/nginx/html/.env

# Expose port 80
EXPOSE 80

# Nginx's default MIME table already serves .wasm as application/wasm.
