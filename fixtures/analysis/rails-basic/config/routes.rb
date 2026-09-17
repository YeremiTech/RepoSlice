Rails.application.routes.draw do
  get '/health', to: 'health#show'
  resources :users
end
