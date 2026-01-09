FROM nginx:alpine

# Copy static assets
COPY . /usr/share/nginx/html

# Expose port 80
EXPOSE 80

# Nginx default config usually includes correct MIME types for WASM,
# but we can ensure it by uncommenting this if needed.
# COPY nginx.conf /etc/nginx/conf.d/default.conf
