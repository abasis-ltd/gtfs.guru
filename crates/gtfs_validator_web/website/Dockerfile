FROM nginx:alpine

# Copy static assets
COPY . /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

# Expose port 80
EXPOSE 80

# Nginx's default MIME table already serves .wasm as application/wasm.
